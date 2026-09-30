FROM source.archipelago-foundation.org/lfg2025/nginx:1.27.4-alpine
# Static site content.
COPY index.html /usr/share/nginx/html/
#
# FIPS UI talks only to the archipelago RPC on 127.0.0.1:5678, using the
# browser's own archipelago session — there is NO per-node secret to
# substitute, so (unlike bitcoin-ui) the nginx config is baked straight
# into the image rather than bind-mounted/rendered at container-create.
COPY nginx.conf /etc/nginx/conf.d/default.conf
#
# Run nginx as root to avoid chown failures in rootless Podman user
# namespaces. The rest of the nginx image is unchanged.
RUN sed -i 's/^user  nginx;/user  root;/' /etc/nginx/nginx.conf && \
    mkdir -p /var/cache/nginx/client_temp /var/cache/nginx/proxy_temp \
             /var/cache/nginx/fastcgi_temp /var/cache/nginx/uwsgi_temp \
             /var/cache/nginx/scgi_temp
EXPOSE 8336
ENTRYPOINT []
CMD ["nginx", "-g", "daemon off;"]
