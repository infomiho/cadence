use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use futures::FutureExt as _;
use gpui::{
    App, AppContext as _, Asset, AssetLogger, Entity, ImageAssetLoader, ImageCache,
    ImageCacheError, ImageLoadingTask, RenderImage, Resource, Task, Window,
};

const MAX_IMAGES: usize = 48;
const MAX_DECODED_BYTES: usize = 32 * 1024 * 1024;
const FAILURE_RETRY_DELAY: Duration = Duration::from_secs(5);

enum CacheEntry {
    Loading {
        image: ImageLoadingTask,
        retry_ready: Arc<AtomicBool>,
        notification: Option<Task<()>>,
    },
    Loaded {
        image: Arc<RenderImage>,
        decoded_bytes: usize,
    },
    Failed {
        error: ImageCacheError,
        retry_ready: Arc<AtomicBool>,
        _notification: Task<()>,
    },
}

impl CacheEntry {
    fn get(&mut self) -> (Option<Result<Arc<RenderImage>, ImageCacheError>>, usize) {
        match self {
            Self::Loading {
                image,
                retry_ready,
                notification,
            } => {
                let Some(result) = image.now_or_never() else {
                    return (None, 0);
                };
                match result {
                    Ok(image) => {
                        let decoded_bytes = (0..image.frame_count())
                            .filter_map(|frame| image.as_bytes(frame))
                            .map(<[u8]>::len)
                            .sum();
                        *self = Self::Loaded {
                            image: image.clone(),
                            decoded_bytes,
                        };
                        (Some(Ok(image)), decoded_bytes)
                    }
                    Err(error) => {
                        *self = Self::Failed {
                            error: error.clone(),
                            retry_ready: retry_ready.clone(),
                            _notification: notification
                                .take()
                                .expect("image notification task disappeared"),
                        };
                        (Some(Err(error)), 0)
                    }
                }
            }
            Self::Loaded { image, .. } => (Some(Ok(image.clone())), 0),
            Self::Failed { error, .. } => (Some(Err(error.clone())), 0),
        }
    }

    fn retry_ready(&self) -> bool {
        matches!(self, Self::Failed { retry_ready, .. } if retry_ready.load(Ordering::Acquire))
    }
}

pub(super) struct BoundedImageCache {
    items: HashMap<Resource, CacheEntry>,
    recency: VecDeque<Resource>,
    decoded_bytes: usize,
    hits: u64,
    misses: u64,
    retries: u64,
    evictions: u64,
}

impl BoundedImageCache {
    pub(super) fn new(cx: &mut App) -> Entity<Self> {
        let cache = cx.new(|_| Self {
            items: HashMap::new(),
            recency: VecDeque::new(),
            decoded_bytes: 0,
            hits: 0,
            misses: 0,
            retries: 0,
            evictions: 0,
        });
        cx.observe_release(&cache, |cache, cx| {
            for item in cache.items.values_mut() {
                if let CacheEntry::Loaded { image, .. } = item {
                    cx.drop_image(image.clone(), None);
                }
            }
        })
        .detach();
        cache
    }

    fn touch(&mut self, resource: &Resource) {
        self.recency.retain(|cached| cached != resource);
        self.recency.push_back(resource.clone());
    }

    fn evict_to_budget(&mut self, reserved_entries: usize, window: &mut Window, cx: &mut App) {
        while exceeds_cache_budget(self.items.len(), reserved_entries, self.decoded_bytes) {
            let Some(resource) = self.recency.pop_front() else {
                break;
            };
            if let Some(item) = self.items.remove(&resource) {
                if let CacheEntry::Loaded {
                    image,
                    decoded_bytes,
                } = item
                {
                    self.decoded_bytes = self.decoded_bytes.saturating_sub(decoded_bytes);
                    cx.drop_image(image, Some(window));
                }
                self.evictions += 1;
            }
        }
    }
}

fn exceeds_cache_budget(item_count: usize, reserved_entries: usize, decoded_bytes: usize) -> bool {
    item_count + reserved_entries > MAX_IMAGES
        || (decoded_bytes > MAX_DECODED_BYTES && item_count > 1)
}

impl ImageCache for BoundedImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        if self
            .items
            .get(resource)
            .is_some_and(CacheEntry::retry_ready)
        {
            self.items.remove(resource);
            self.recency.retain(|cached| cached != resource);
            self.retries += 1;
        }
        if self.items.contains_key(resource) {
            self.hits += 1;
            self.touch(resource);
            let (result, loaded_bytes) = self
                .items
                .get_mut(resource)
                .expect("cached image disappeared")
                .get();
            if loaded_bytes > 0 {
                self.decoded_bytes += loaded_bytes;
                self.evict_to_budget(0, window, cx);
            }
            return result;
        }

        self.misses += 1;
        self.evict_to_budget(1, window, cx);
        let future = AssetLogger::<ImageAssetLoader>::load(resource.clone(), cx);
        let image = cx.background_executor().spawn(future).shared();
        let retry_ready = Arc::new(AtomicBool::new(false));
        let entity = window.current_view();
        let notification = window.spawn(cx, {
            let image = image.clone();
            let retry_ready = retry_ready.clone();
            async move |cx| {
                let failed = image.await.is_err();
                cx.on_next_frame(move |_, cx| cx.notify(entity));
                if failed {
                    cx.background_executor().timer(FAILURE_RETRY_DELAY).await;
                    retry_ready.store(true, Ordering::Release);
                    cx.on_next_frame(move |_, cx| cx.notify(entity));
                }
            }
        });
        self.items.insert(
            resource.clone(),
            CacheEntry::Loading {
                image,
                retry_ready,
                notification: Some(notification),
            },
        );
        self.touch(resource);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(name: &'static str) -> Resource {
        Resource::Embedded(name.into())
    }

    #[test]
    fn cache_budget_combines_byte_and_entry_limits() {
        assert!(!exceeds_cache_budget(1, 0, MAX_DECODED_BYTES + 1));
        assert!(exceeds_cache_budget(2, 0, MAX_DECODED_BYTES + 1));
        assert!(exceeds_cache_budget(MAX_IMAGES, 1, 0));
    }

    #[test]
    fn touching_an_item_moves_it_to_the_lru_tail() {
        let first = resource("first");
        let second = resource("second");
        let mut cache = BoundedImageCache {
            items: HashMap::new(),
            recency: VecDeque::from([first.clone(), second.clone()]),
            decoded_bytes: 0,
            hits: 0,
            misses: 0,
            retries: 0,
            evictions: 0,
        };

        cache.touch(&first);

        assert_eq!(cache.recency, VecDeque::from([second, first]));
    }
}
