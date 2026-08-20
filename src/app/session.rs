use super::*;

/// The Spotify account this instance is signed in to, and how far along the
/// connect-and-authorize sequence it has got.
///
/// Owned by the services global rather than a window: signing in is not
/// something the listener should have to redo because a window went away.
pub(super) struct Session {
    backend: BackendHandle,
    /// Bumped whenever the account changes, so replies to the previous account
    /// can be recognised and dropped.
    generation: u64,
    state: ConnectionState,
    client_id: Option<String>,
    client_id_source: Option<ClientIdSource>,
    setup_error: Option<String>,
    setup_needs_focus: bool,
    configuration_request_id: u64,
    pending_configuration: Option<u64>,
    configuration_blocked: bool,
    app_change_confirmation_open: bool,
    /// Where to return if restarting Spotify setup fails before it begins.
    state_before_app_change: Option<ConnectionState>,
    /// The last failure, kept here so a sign-in window opened by the very
    /// event that carried it can still show it.
    last_failure: Option<String>,
    profile: Option<model::UserProfile>,
}

/// Things the surrounding app has to react to when the account changes.
pub(super) enum SessionEvent {
    /// The account went away; every cached view of it must be discarded.
    LoggedOut,
    /// The catalog is loaded and usable again.
    Ready,
    /// A new account is being loaded, so navigation should start from the top.
    Restarted,
    Failed(String),
    Notice(String),
}

impl EventEmitter<SessionEvent> for Session {}

impl Session {
    pub(super) fn new(backend: BackendHandle) -> Self {
        Self {
            backend,
            generation: 0,
            state: ConnectionState::Starting,
            client_id: None,
            client_id_source: None,
            setup_error: None,
            setup_needs_focus: true,
            configuration_request_id: 0,
            pending_configuration: None,
            configuration_blocked: false,
            app_change_confirmation_open: false,
            state_before_app_change: None,
            last_failure: None,
            profile: None,
        }
    }

    /// Puts the account back into its opening state after the worker restarted.
    pub(super) fn restarted(&mut self, cx: &mut Context<Self>) {
        self.state = ConnectionState::Starting;
        cx.notify();
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn state(&self) -> &ConnectionState {
        &self.state
    }

    pub(super) fn is_ready(&self) -> bool {
        matches!(self.state, ConnectionState::Ready)
    }

    pub(super) fn client_id(&self) -> Option<&String> {
        self.client_id.as_ref()
    }

    pub(super) fn client_id_source(&self) -> Option<ClientIdSource> {
        self.client_id_source
    }

    pub(super) fn setup_error(&self) -> Option<&String> {
        self.setup_error.as_ref()
    }

    pub(super) fn configuration_blocked(&self) -> bool {
        self.configuration_blocked
    }

    pub(super) fn app_change_confirmation_open(&self) -> bool {
        self.app_change_confirmation_open
    }

    pub(super) fn profile(&self) -> Option<&model::UserProfile> {
        self.profile.as_ref()
    }

    /// Reports whether the setup field still needs focus, clearing the request.
    pub(super) fn take_setup_focus(&mut self) -> bool {
        std::mem::take(&mut self.setup_needs_focus)
    }

    pub(super) fn set_connecting(&mut self, cx: &mut Context<Self>) {
        self.state = ConnectionState::Connecting;
        self.last_failure = None;
        cx.notify();
    }

    pub(super) fn authenticate(&mut self, cx: &mut Context<Self>) {
        self.last_failure = None;
        let generation = next_request_id(&mut self.generation);
        self.send(BackendCommand::Authenticate { generation }, cx);
        cx.emit(SessionEvent::Restarted);
        cx.notify();
    }

    pub(super) fn configure(&mut self, client_id: String, cx: &mut Context<Self>) -> bool {
        self.setup_error = None;
        self.last_failure = None;
        self.state = ConnectionState::Connecting;
        let generation = next_request_id(&mut self.configuration_request_id);
        self.pending_configuration = Some(generation);
        let sent = self.send(
            BackendCommand::ConfigureSpotify {
                generation,
                client_id,
            },
            cx,
        );
        if !sent {
            self.pending_configuration = None;
            self.state = ConnectionState::SetupRequired;
        }
        cx.notify();
        sent
    }

    pub(super) fn reject_client_id(&mut self, error: impl Into<String>, cx: &mut Context<Self>) {
        self.setup_error = Some(error.into());
        cx.notify();
    }

    pub(super) fn clear_setup_error(&mut self, cx: &mut Context<Self>) {
        self.setup_error = None;
        cx.notify();
    }

    pub(super) fn request_app_change(&mut self, cx: &mut Context<Self>) {
        self.app_change_confirmation_open = true;
        cx.notify();
    }

    pub(super) fn cancel_app_change(&mut self, cx: &mut Context<Self>) {
        self.app_change_confirmation_open = false;
        cx.notify();
    }

    pub(super) fn confirm_app_change(&mut self, cx: &mut Context<Self>) {
        self.app_change_confirmation_open = false;
        let previous = self.state;
        self.state = ConnectionState::Connecting;
        let generation = next_request_id(&mut self.generation);
        cx.emit(SessionEvent::Restarted);
        if self.send(BackendCommand::ResetSpotifyConfiguration { generation }, cx) {
            self.state_before_app_change = Some(previous);
        } else {
            self.state = previous;
            self.fail("Unable to restart Spotify setup.", cx);
        }
        cx.notify();
    }

    pub(super) fn logout(&mut self, cx: &mut Context<Self>) {
        self.app_change_confirmation_open = false;
        let generation = next_request_id(&mut self.generation);
        cx.emit(SessionEvent::Restarted);
        self.send(BackendCommand::Logout { generation }, cx);
        cx.notify();
    }

    pub(super) fn last_failure(&self) -> Option<&String> {
        self.last_failure.as_ref()
    }

    fn fail(&mut self, error: impl Into<String>, cx: &mut Context<Self>) {
        let error = error.into();
        self.last_failure = Some(error.clone());
        cx.emit(SessionEvent::Failed(error));
    }

    fn send(&mut self, command: BackendCommand, cx: &mut Context<Self>) -> bool {
        if self.backend.send(command) {
            return true;
        }
        self.fail("Cadence backend is busy or not running", cx);
        false
    }

    /// Applies the account half of a backend event, returning the event when the
    /// surrounding app still has its own work to do for it.
    pub(super) fn handle_backend_event(
        &mut self,
        event: BackendEvent,
        cx: &mut Context<Self>,
    ) -> Option<BackendEvent> {
        match event {
            BackendEvent::SetupRequired => {
                self.state = ConnectionState::SetupRequired;
                self.state_before_app_change = None;
                self.client_id = None;
                self.client_id_source = None;
                self.setup_needs_focus = true;
                self.configuration_blocked = false;
            }
            BackendEvent::SpotifyConfigured {
                generation,
                client_id,
                source,
            } => {
                self.client_id = Some(client_id);
                self.client_id_source = Some(source);
                self.configuration_blocked = false;
                if self.pending_configuration == Some(generation) {
                    self.pending_configuration = None;
                    self.state = ConnectionState::Connecting;
                    self.authenticate(cx);
                }
            }
            BackendEvent::SpotifyConfigurationFailed { generation, error } => {
                if generation == 0 && self.client_id_source == Some(ClientIdSource::Environment) {
                    self.state = ConnectionState::AuthorizationRequired;
                    self.configuration_blocked = true;
                    self.fail(error, cx);
                } else if generation == 0 || self.pending_configuration == Some(generation) {
                    self.pending_configuration = None;
                    self.state = ConnectionState::SetupRequired;
                    self.setup_error = Some(format!(
                        "Could not configure Spotify. Check the Client ID and try again. {error}"
                    ));
                    self.setup_needs_focus = true;
                }
            }
            BackendEvent::SpotifyConfigurationResetFailed(error) => {
                // Return to wherever the change started; assuming Ready would
                // fake a working session out of a first-run setup screen.
                if let Some(previous) = self.state_before_app_change.take() {
                    self.state = previous;
                }
                cx.emit(SessionEvent::Notice(format!(
                    "Unable to restart Spotify setup. Check your connection and try again. {error}"
                )));
            }
            BackendEvent::AuthorizationRequired => {
                if !matches!(self.state, ConnectionState::Connecting) {
                    self.state = ConnectionState::AuthorizationRequired;
                }
            }
            BackendEvent::LoggedOut => {
                self.state = ConnectionState::AuthorizationRequired;
                self.profile = None;
                cx.emit(SessionEvent::LoggedOut);
            }
            BackendEvent::CatalogReady { generation } => {
                if generation == self.generation {
                    self.state = ConnectionState::Ready;
                    self.last_failure = None;
                    cx.emit(SessionEvent::Ready);
                }
            }
            BackendEvent::ProfileLoaded {
                generation,
                profile,
            } => {
                if generation == self.generation {
                    self.profile = Some(profile);
                }
            }
            BackendEvent::AuthorizationFailed(error) => {
                self.state = ConnectionState::AuthorizationRequired;
                self.fail(error, cx);
            }
            BackendEvent::FatalError(error) => {
                self.state = ConnectionState::Failed;
                self.fail(error, cx);
            }
            BackendEvent::CatalogFailed { generation, error } => {
                if generation == self.generation {
                    self.state = ConnectionState::Ready;
                }
                cx.notify();
                return Some(BackendEvent::CatalogFailed { generation, error });
            }
            event => return Some(event),
        }
        cx.notify();
        None
    }
}
