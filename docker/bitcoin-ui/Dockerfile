FROM git.tx1138.com/lfg2025/nginx:1.27.4-alpine
# Static site content.
COPY index.html /usr/share/nginx/html/
COPY tailwind.css /usr/share/nginx/html/
COPY qrcode.js /usr/share/nginx/html/
COPY 50x.html /usr/share/nginx/html/
COPY assets/ /usr/share/nginx/html/assets/
#
# NOTE: /etc/nginx/conf.d/default.conf is intentionally NOT copied from
# this build context. It is bind-mounted at container-create time from
# /var/lib/archipelago/bitcoin-ui/nginx.conf on the host, which the
# archipelago prod orchestrator renders with the current base64 RPC
# auth substituted in (see core/archipelago/src/container/bitcoin_ui.rs).
#
# If the bind-mount fails nginx will start with no site configured and
# return 404 on every request. That's the intended safe failure mode —
# better than baking a placeholder into the image and potentially
# serving the upstream RPC proxy with a stale/empty Authorization header.
#
# Run nginx as root to avoid chown failures in rootless Podman user
# namespaces. The rest of the nginx image is unchanged.
RUN sed -i 's/^user  nginx;/user  root;/' /etc/nginx/nginx.conf && \
    mkdir -p /var/cache/nginx/client_temp /var/cache/nginx/proxy_temp \
             /var/cache/nginx/fastcgi_temp /var/cache/nginx/uwsgi_temp \
             /var/cache/nginx/scgi_temp
EXPOSE 8334
ENTRYPOINT []
CMD ["nginx", "-g", "daemon off;"]
