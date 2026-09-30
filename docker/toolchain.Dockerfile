# Minimal-glibc toolchain container (ubuntu:24.04 / glibc 2.39 — the lowest
# glibc the release artifacts target).
#
# At build time this installs the Mire toolchain (compiler + owl + kioto +
# mire stdlib) straight from the release tarballs published for the tag, so
# the image is self-contained and reproducible. Images are tagged with the
# target triple so the installer's `--docker` mode can pull them directly
# (DOCKER_IMAGE default: mire-lang/toolchain:<arch>).
#
#   docker build --build-arg TARGET_TRIPLE=x86_64-unknown-linux-gnu .
#   docker run --rm -it mire-lang/toolchain:x86_64-unknown-linux-gnu
#       -v "$(pwd)":/workspace
#       -w /workspace  /usr/local/bin/owl run

FROM ubuntu:24.04

ARG TARGET_TRIPLE=x86_64-unknown-linux-gnu

ENV DEBIAN_FRONTEND=noninteractive
ENV OWL_HOME=/root/.owl

COPY install/install.sh /opt/mire-install/install.sh

# The installer's install_deps step provisions clang/llc/lld/openssl/etc. via
# apt (it runs with --yes here); we only need the network plumbing up front.
RUN apt-get update -qq \
    && apt-get install -y -qq --no-install-recommends \
        curl ca-certificates tar xz-utils git \
    && rm -rf /var/lib/apt/lists/* \
    && sh /opt/mire-install/install.sh \
        --yes --no-profile --prefix /usr/local \
        --arch "$TARGET_TRIPLE" --compiler \
    && rm -rf /opt/mire-install /tmp/mire-* 2>/dev/null || true \
    && mire --version \
    && owl --version

WORKDIR /workspace

CMD ["/usr/local/bin/mire", "--help"]