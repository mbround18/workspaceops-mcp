CARGO ?= cargo

.DEFAULT_GOAL := help

.PHONY: help
help: ## List targets
	@grep -hE '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "} {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

.PHONY: install
install: ## Build and install workspaceops + workspaceops-mcp
	$(CARGO) install --path . --locked --force

.PHONY: test
test: ## Run tests
	$(CARGO) test

.PHONY: fmt
fmt: ## Format project
	$(CARGO) fmt
