TARGET = aarch64-unknown-linux-musl
RELEASE_DIR = target/$(TARGET)/release
LAMBDA_DIR = target/lambda
CRATES = forecast geocode metadata stations cache_warmer

.PHONY: all build package test clean

all: build package

build:
	cargo zigbuild --release --target $(TARGET)

package: $(LAMBDA_DIR)
	@for crate in $(CRATES); do \
		cp $(RELEASE_DIR)/$$crate bootstrap && \
		zip $(LAMBDA_DIR)/$$crate.zip bootstrap && \
		rm bootstrap; \
	done

$(LAMBDA_DIR):
	mkdir -p $(LAMBDA_DIR)

test:
	cargo test --workspace

clean:
	cargo clean
	rm -rf $(LAMBDA_DIR)
