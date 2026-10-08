document.querySelectorAll('[data-preview-group]').forEach((group) => {
  group.querySelectorAll('button').forEach((button) => {
    button.addEventListener('click', () => {
      group.querySelectorAll('button').forEach((option) => option.classList.remove('selected'));
      button.classList.add('selected');
    });
  });
});

document.querySelectorAll('[data-preview-cta]').forEach((button) => {
  button.addEventListener('click', () => {
    const original = button.innerHTML;
    button.innerHTML = 'Preview only · ready to activate <span>✓</span>';
    button.classList.add('preview-confirmed');
    window.setTimeout(() => {
      button.innerHTML = original;
      button.classList.remove('preview-confirmed');
    }, 1800);
  });
});
