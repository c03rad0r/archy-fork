// Keep the unchanged Business UI's API requests in its isolated app namespace.
const businessFetch = window.fetch.bind(window);
window.fetch = (input, init) => {
  const url = new URL(input instanceof Request ? input.url : input, location.href);
  if (url.origin === location.origin && url.pathname.startsWith('/api/')) {
    url.pathname = '/business-demo' + url.pathname;
    input = input instanceof Request ? new Request(url, input) : url;
  }
  return businessFetch(input, init);
};
