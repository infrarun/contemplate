FROM --platform=$BUILDPLATFORM alpine AS build
ARG TARGETPLATFORM
ARG BUILDPLATFORM
ADD dist /dist
RUN if [[ "${TARGETPLATFORM}" == "linux/amd64" ]]; then \
        cp /dist/x86_64-unknown-linux-musl/contemplate-x86_64-unknown-linux-musl /contemplate; \
    elif [[ "${TARGETPLATFORM}" == "linux/arm64" ]]; then \
        cp /dist/aarch64-unknown-linux-musl/contemplate-aarch64-unknown-linux-musl /contemplate; \
    elif [[ "${TARGETPLATFORM}" == "linux/386" ]]; then \
        cp /dist/i686-unknown-linux-musl/contemplate-i686-unknown-linux-musl /contemplate; \
    fi
FROM scratch
COPY --from=build --chmod=755 /contemplate /contemplate
LABEL org.opencontainers.image.title="Contemplate"
LABEL org.opencontainers.image.base.name="infrarun/contemplate"
LABEL org.opencontainers.image.source="https://github.com/infrarun/contemplate"
LABEL org.opencontainers.image.authors="infra.run"
LABEL org.opencontainers.image.url="https://infrarun.github.io/contemplate/"
LABEL org.opencontainers.image.documentation="https://infrarun.github.io/contemplate/"
LABEL org.opencontainers.image.vendor="infra.run"
LABEL org.opencontainers.image.licenses="MIT"
ENTRYPOINT [ "/contemplate" ]
