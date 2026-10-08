FROM docker.io/library/python:3.13-alpine@sha256:2d9aefe2fef018a7eb2c13064c89c71929800fd2e5dccdbf52ea5da5bb8d929a
RUN apk add --no-cache nodejs
WORKDIR /app
COPY server.py payments.py boot.py ./
COPY business ./business
COPY public ./public
COPY UPSTREAM-LICENSE /usr/share/licenses/justworks/LICENSE
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
USER 65534:65534
EXPOSE 8340
CMD ["python3", "boot.py"]
