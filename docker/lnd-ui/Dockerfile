FROM source.archipelago-foundation.org/lfg2025/nginx:1.27.4-alpine

# Copy the HTML file
COPY index.html /usr/share/nginx/html/
COPY tailwind.css /usr/share/nginx/html/
COPY qrcode.js /usr/share/nginx/html/

# Create directories for assets
RUN mkdir -p /usr/share/nginx/html/assets/img/app-icons && \
    mkdir -p /usr/share/nginx/html/assets/img

# Copy assets
# Canonical app icon — the same file the app store / My Apps renders
# (neode-ui/public/assets/img/app-icons/lnd.png, referenced by the signed
# catalog as /assets/img/app-icons/lnd.png). Keep these in sync.
COPY lnd.png /usr/share/nginx/html/assets/img/app-icons/
COPY bg-web5.jpg /usr/share/nginx/html/assets/img/
COPY bg-intro.jpg /usr/share/nginx/html/assets/img/

# Copy nginx config
COPY nginx.conf /etc/nginx/conf.d/default.conf

# Run nginx as root to avoid chown failures in rootless Podman user namespaces
RUN sed -i 's/^user  nginx;/user  root;/' /etc/nginx/nginx.conf && \
    mkdir -p /var/cache/nginx/client_temp /var/cache/nginx/proxy_temp \
             /var/cache/nginx/fastcgi_temp /var/cache/nginx/uwsgi_temp \
             /var/cache/nginx/scgi_temp
EXPOSE 18083
ENTRYPOINT []
CMD ["nginx", "-g", "daemon off;"]
