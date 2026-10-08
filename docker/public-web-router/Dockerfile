FROM docker.io/library/python:3.13-slim-bookworm@sha256:a1165e272e578941b84abc79e4ab38a0305cd12803a5c4247979ac7655f4d641
COPY download.py /build/download.py
RUN python3 /build/download.py && rm -rf /build
COPY router.py /app/router.py
ENV XDG_DATA_HOME=/data XDG_CONFIG_HOME=/data/config PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
ENTRYPOINT ["python3", "/app/router.py"]
