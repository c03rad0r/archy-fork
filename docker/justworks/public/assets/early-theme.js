try {
  const theme = localStorage.getItem('jw-theme');
  if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
} catch { /* System theme also works without storage. */ }
