FROM node:22-alpine AS build
WORKDIR /src
COPY package.json vite.config.ts index.html ./
COPY src ./src
RUN npm install --ignore-scripts && npm run build

FROM nginx:1.27-alpine
COPY --from=build /src/dist /usr/share/nginx/html
COPY <<'EOF' /etc/nginx/conf.d/default.conf
server {
  listen 8080;
  root /usr/share/nginx/html;
  index index.html;
  location / { try_files $uri $uri/ /index.html; }
}
EOF
