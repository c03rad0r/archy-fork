# IndeedHub demo — clone, build, and serve in one step
# No submodule or local source needed
FROM node:20-alpine AS builder
RUN apk add --no-cache git
WORKDIR /app
RUN git clone --depth 1 https://github.com/Zazawowow/indeehub-archipelago.git .
RUN npm ci
ENV VITE_USE_MOCK_DATA=false
ENV VITE_INDEEHUB_API_URL=/api
ENV VITE_INDEEHUB_CDN_URL=/storage
ENV VITE_NOSTR_RELAYS=""
RUN npm run build

FROM nginx:alpine
RUN apk add --no-cache curl
COPY --from=builder /app/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html
EXPOSE 7777
CMD ["nginx", "-g", "daemon off;"]
