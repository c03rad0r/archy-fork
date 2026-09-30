FROM docker.io/library/nginx:1.31.3-alpine@sha256:1d40e3eb3bf4f138de1d67193f2aa5309fcaf343eb5ffadbf5e9439de1eb1ebb
COPY nginx.conf /etc/angor-nginx.conf.template
COPY entrypoint.sh /usr/local/bin/angor-indexer
USER 101:101
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/angor-indexer"]
