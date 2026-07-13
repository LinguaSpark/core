FROM buildpack-deps:oldstable-curl

ARG TARGETARCH

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install -y cmake libpcre2-dev pkg-config && \
    rm -rf /var/lib/apt/lists/*

RUN apt-get update && \
    apt-get install -y build-essential git && \
    rm -rf /var/lib/apt/lists/*

RUN if [ "$TARGETARCH" = "amd64" ]; then \
      wget -qO- https://apt.repos.intel.com/intel-gpg-keys/GPG-PUB-KEY-INTEL-SW-PRODUCTS.PUB | \
        gpg --dearmor -o /usr/share/keyrings/oneapi-archive-keyring.gpg && \
      echo "deb [signed-by=/usr/share/keyrings/oneapi-archive-keyring.gpg] https://apt.repos.intel.com/oneapi all main" > /etc/apt/sources.list.d/oneAPI.list && \
      apt-get update && \
      apt-get install -y liblapack-dev libblas-dev libopenblas-dev intel-oneapi-mkl-devel && \
      rm -rf /var/lib/apt/lists/*; \
    elif [ "$TARGETARCH" != "arm64" ]; then \
      echo "Unsupported architecture: $TARGETARCH" >&2; \
      exit 1; \
    fi

ENV MKLROOT=/opt/intel/oneapi/mkl/latest

WORKDIR /app/linguaspark
COPY . /app

RUN if [ "$TARGETARCH" = "amd64" ]; then \
      CMAKE_ARCH_ARGS="-DUSE_MKL=ON -DBUILD_ARCH=x86-64"; \
    else \
      CMAKE_ARCH_ARGS="-DUSE_MKL=OFF -DBUILD_ARCH=armv8-a"; \
    fi && \
    cmake -B build -S . \
          -DCMAKE_BUILD_TYPE=Release \
          $CMAKE_ARCH_ARGS \
          -DCMAKE_CXX_FLAGS_RELEASE="-Wno-deprecated-declarations" && \
    cmake --build build -j4 && \
    mkdir -p /build && \
    cp build/liblinguaspark.so /build/ && \
    if [ "$TARGETARCH" = "amd64" ]; then \
      cp /opt/intel/oneapi/compiler/latest/lib/libiomp5.so /build/; \
    fi
