APP_NAME ?= ai-gateway
REGISTRY ?= ghcr.io
IMAGE_REPO ?= your-org/$(APP_NAME)
IMAGE ?= $(REGISTRY)/$(IMAGE_REPO)
TAG ?= latest
PORT ?= 8080
CONTAINER_NAME ?= $(APP_NAME)
ENV_FILE ?= .env
DOCKER_BUILDER ?= $(shell docker context show 2>/dev/null || echo default)
DOCKER_BUILD_FLAGS ?= --load
DOCKER_RUN_FLAGS ?=
PREFIX ?= /usr/local
BIN_DIR ?= $(PREFIX)/bin

.PHONY: help run restart ensure-env docker-check docker-build docker-run docker-stop docker-logs docker-shell docker-tag docker-push docker-release ctl-build ctl-release ctl-install

help:
	@echo "Available targets:"
	@echo "  make run            Build and start the app in Docker"
	@echo "  make restart        Rebuild and restart the app in Docker"
	@echo "  make docker-build   IMAGE=<registry/repo> TAG=<tag> DOCKER_BUILDER=<builder>"
	@echo "  make docker-run     ENV_FILE=.env PORT=8080 CONTAINER_NAME=ai-gateway"
	@echo "  make docker-stop    CONTAINER_NAME=ai-gateway"
	@echo "  make docker-logs    CONTAINER_NAME=ai-gateway"
	@echo "  make docker-shell   CONTAINER_NAME=ai-gateway"
	@echo "  make docker-tag     IMAGE=<registry/repo> TAG=<tag> NEW_TAG=<tag>"
	@echo "  make docker-push    IMAGE=<registry/repo> TAG=<tag>"
	@echo "  make docker-release IMAGE=<registry/repo> TAG=<tag>"
	@echo "  make ctl-build"
	@echo "  make ctl-release"
	@echo "  make ctl-install   PREFIX=/usr/local"

run restart: IMAGE := $(APP_NAME)
run restart: ensure-env docker-build docker-stop docker-run

ensure-env:
	@if [ ! -f "$(ENV_FILE)" ]; then \
		cp .env.example "$(ENV_FILE)"; \
		echo "Created $(ENV_FILE) from .env.example"; \
	fi

docker-check:
	@docker version --format '{{.Server.Version}}' >/dev/null 2>&1 || { \
		echo "Docker daemon is not available. Start or restart Docker Desktop and retry."; \
		exit 1; \
	}

docker-build: docker-check
	docker buildx build --builder $(DOCKER_BUILDER) $(DOCKER_BUILD_FLAGS) -t $(IMAGE):$(TAG) .

docker-run: docker-check ensure-env
	docker run -d \
		--name $(CONTAINER_NAME) \
		--restart unless-stopped \
		--env-file $(ENV_FILE) \
		-p $(PORT):8080 \
		-v $(CONTAINER_NAME)-pgdata:/var/lib/postgresql/data \
		$(DOCKER_RUN_FLAGS) \
		$(IMAGE):$(TAG)

docker-stop:
	-docker stop $(CONTAINER_NAME)
	-docker rm $(CONTAINER_NAME)

docker-logs:
	docker logs -f $(CONTAINER_NAME)

docker-shell:
	docker exec -it $(CONTAINER_NAME) bash

docker-tag:
ifndef NEW_TAG
	$(error NEW_TAG is required, example: make docker-tag IMAGE=$(IMAGE) TAG=$(TAG) NEW_TAG=v1.0.0)
endif
	docker tag $(IMAGE):$(TAG) $(IMAGE):$(NEW_TAG)

docker-push:
	docker push $(IMAGE):$(TAG)

docker-release: docker-build docker-push

ctl-build:
	cargo build --bin agctl

ctl-release:
	cargo build --release --bin agctl

ctl-install: ctl-release
	install -d $(BIN_DIR)
	install -m 755 target/release/agctl $(BIN_DIR)/agctl
