.PHONY: test
test:
	cargo llvm-cov --lcov --output-path lcov.info
