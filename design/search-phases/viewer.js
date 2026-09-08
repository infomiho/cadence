const toc = document.querySelector('details.toc');
const narrow = matchMedia('(max-width: 899px)');
const syncNavigation = () => { toc.open = !narrow.matches; };
syncNavigation();
narrow.addEventListener('change', syncNavigation);
toc.querySelectorAll('a').forEach(link => link.addEventListener('click', () => {
  if (narrow.matches) toc.open = false;
}));

const dialog = document.getElementById('lightbox');
const image = document.getElementById('zoom-image');
const caption = document.getElementById('zoom-caption');
let trigger;
document.querySelectorAll('[data-zoom]').forEach(button => {
  button.addEventListener('click', () => {
    trigger = button;
    const source = button.querySelector('img');
    image.src = source.src;
    image.alt = source.alt;
    caption.textContent = button.dataset.caption;
    dialog.showModal();
    document.body.style.overflow = 'hidden';
  });
});
document.getElementById('zoom-close').addEventListener('click', () => dialog.close());
dialog.addEventListener('click', event => {
  const bounds = dialog.getBoundingClientRect();
  if (event.target === dialog && (event.clientX < bounds.left || event.clientX > bounds.right ||
      event.clientY < bounds.top || event.clientY > bounds.bottom)) dialog.close();
});
dialog.addEventListener('close', () => {
  document.body.style.overflow = '';
  trigger?.focus({ preventScroll: true });
});
