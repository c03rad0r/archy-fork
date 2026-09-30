FROM source.archipelago-foundation.org/lfg2025/nginx:1.27.4-alpine

COPY index.html /usr/share/nginx/html/index.html
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY assets/img/bg-network.jpg /usr/share/nginx/html/assets/img/bg-network.jpg
COPY assets/img/app-icons/fedimint.png /usr/share/nginx/html/assets/img/app-icons/fedimint.png
COPY assets/img/app-icons/fedimint.jpg /usr/share/nginx/html/assets/img/app-icons/fedimint.jpg

RUN sed -i 's/^user  nginx;/user  root;/' /etc/nginx/nginx.conf && \
    mkdir -p /var/cache/nginx/client_temp /var/cache/nginx/proxy_temp \
             /var/cache/nginx/fastcgi_temp /var/cache/nginx/uwsgi_temp \
             /var/cache/nginx/scgi_temp

EXPOSE 8175
ENTRYPOINT []
CMD ["nginx", "-g", "daemon off;"]
