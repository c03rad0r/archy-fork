FROM source.archipelago-foundation.org/lfg2025/nginx:1.27.4-alpine
# Static site content.
COPY index.html /usr/share/nginx/html/
COPY 50x.html /usr/share/nginx/html/
COPY assets/ /usr/share/nginx/html/assets/
# Unlike bitcoin-ui, the nginx.conf is baked into the image, not
# bind-mounted: there is no secret to render in. Cuprate's restricted RPC
# (the only upstream this UI proxies) is unauthenticated by design —
# Monero's safe-for-public subset — so there is nothing to substitute at
# start time and no rotation to follow.
COPY nginx.conf /etc/nginx/conf.d/default.conf
#
# Run nginx as root to avoid chown failures in rootless Podman user
# namespaces. The rest of the nginx image is unchanged.
RUN sed -i 's/^user  nginx;/user  root;/' /etc/nginx/nginx.conf && \
    mkdir -p /var/cache/nginx/client_temp /var/cache/nginx/proxy_temp \
             /var/cache/nginx/fastcgi_temp /var/cache/nginx/uwsgi_temp \
             /var/cache/nginx/scgi_temp
EXPOSE 18091
ENTRYPOINT []
CMD ["nginx", "-g", "daemon off;"]
