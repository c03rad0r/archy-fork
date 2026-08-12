FROM git.tx1138.com/lfg2025/nginx:1.27.4-alpine
COPY index.html /usr/share/nginx/html/
COPY 50x.html /usr/share/nginx/html/
COPY qrcode.js /usr/share/nginx/html/
COPY nginx.conf /etc/nginx/conf.d/default.conf
# Run nginx as root to avoid chown failures in rootless Podman user namespaces
RUN sed -i 's/^user  nginx;/user  root;/' /etc/nginx/nginx.conf && \
    mkdir -p /var/cache/nginx/client_temp /var/cache/nginx/proxy_temp \
             /var/cache/nginx/fastcgi_temp /var/cache/nginx/uwsgi_temp \
             /var/cache/nginx/scgi_temp
EXPOSE 50002
ENTRYPOINT []
CMD ["nginx", "-g", "daemon off;"]
