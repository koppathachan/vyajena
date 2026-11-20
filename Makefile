# Local development shortcuts: a server, a stand-in relay, the iOS Simulator and an
# Android emulator. `make help` lists them. Nothing here touches the real server or
# the GCP relay (deploy/server and deploy/relay do that, on purpose and by hand).
#
# Typical session, one terminal each:
#   make server        (or: make server-relayed, and make relay)
#   make ios
#   make android
# The first phone joins with the bootstrap invite the server prints on its very
# first start (README: "Run the app locally").

SHELL := /bin/bash
.DEFAULT_GOAL := help

SERVER_PORT       ?= 8443
RELAY_PORT    ?= 9443
ANDROID_HOME  ?= $(HOME)/Library/Android/sdk
ADB           ?= $(shell command -v adb || echo $(ANDROID_HOME)/platform-tools/adb)
# `make emulators` lists ids; `make devices` lists running ones.
ANDROID_EMULATOR ?=
ANDROID_DEVICE   ?= emulator-5554
# Defaults to the booted simulator; pass IOS_DEVICE=<id> to pick one.
IOS_DEVICE    ?= $(shell xcrun simctl list devices booted 2>/dev/null | grep -Eo '[0-9A-F]{8}-([0-9A-F]{4}-){3}[0-9A-F]{12}' | head -1)

# Git-ignored with the rest of server/.server-local/.
RELAYED_DIR := server/.server-local/relayed

.PHONY: help server-relayed relay ios ios-sim android android-emulator android-reverse \
        server-reset server-relayed-reset reset-ios reset-android rust-targets devices emulators \
        test-ios test-android test-bridge

help: ## List the targets
	@grep -hE '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | sed -E 's/^([a-z-]+):.*## (.*)/  make \1\t\2/' | expand -t 24

# --- the server and the relay ----------------------------------------------------

server: ## Local server on 127.0.0.1:8443 (server/local.toml); the invite prints on first start only
	cargo run -p server -- --config server/local.toml

# Rewritten on every run, so SERVER_PORT and RELAY_PORT overrides take effect.
.PHONY: $(RELAYED_DIR)/server.toml
$(RELAYED_DIR)/server.toml:
	mkdir -p $(RELAYED_DIR)/blobs
	printf '%s\n' \
	  'server_id = "chat"' \
	  'admin_username = "akhil"' \
	  'listen_address = "127.0.0.1:$(SERVER_PORT)"' \
	  'public_address = "127.0.0.1:$(RELAY_PORT)"' \
	  'database_path = "server.sqlite"' \
	  'key_dir = "keys"' \
	  '' \
	  '[blob_store]' \
	  'backend = "filesystem"' \
	  'path = "blobs"' > $@

server-relayed: $(RELAYED_DIR)/server.toml ## Local server whose invites name the relay port, own data; needs `make relay`
	cargo run -p server -- --config $<

relay: ## Stand-in relay: TCP passthrough 127.0.0.1:9443 -> the server (not rathole)
	python3 deploy/local/relay.py $(RELAY_PORT) $(SERVER_PORT)

server-reset: ## Delete the local server's data (new keys, new bootstrap invite); stop it first
	rm -rf server/.server-local/server.sqlite* server/.server-local/keys

server-relayed-reset: ## Same for the relayed server; stop it first
	rm -rf $(RELAYED_DIR)

# --- the phones --------------------------------------------------------------

APP_ID := com.purplechaamba.vyajena


rust-targets: ## One-time: the Rust targets cargokit builds the bridge for
	rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios \
	  aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

devices: ## flutter devices
	cd app && flutter devices

emulators: ## flutter emulators (ids for ANDROID_EMULATOR=)
	cd app && flutter emulators

ios-sim:
	@xcrun simctl list devices booted | grep -q Booted || open -a Simulator \
	  || open "$$(xcode-select -p)/Applications/Simulator.app"
	@for _ in $$(seq 1 90); do xcrun simctl list devices booted | grep -q Booted && exit 0; sleep 1; done; \
	  echo "no iOS Simulator booted; open one in Simulator" >&2; exit 1

ios: ios-sim ## Run the app on the booted iOS Simulator (it reaches the Mac's 127.0.0.1 directly)
	cd app && flutter run -d $(IOS_DEVICE)

android-emulator:
	@if "$(ADB)" devices | grep -q '^$(ANDROID_DEVICE)[[:space:]]*device'; then exit 0; fi; \
	if [ -z "$(ANDROID_EMULATOR)" ]; then \
	  echo "no Android emulator running: make android ANDROID_EMULATOR=<id> (ids: make emulators)" >&2; exit 1; fi; \
	(cd app && flutter emulators --launch $(ANDROID_EMULATOR)); \
	"$(ADB)" wait-for-device; \
	until [ "$$("$(ADB)" -s $(ANDROID_DEVICE) shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; do sleep 2; done

android-reverse: android-emulator ## adb reverse the server and relay ports (emulator 127.0.0.1 -> the Mac)
	"$(ADB)" -s $(ANDROID_DEVICE) reverse tcp:$(SERVER_PORT) tcp:$(SERVER_PORT)
	"$(ADB)" -s $(ANDROID_DEVICE) reverse tcp:$(RELAY_PORT) tcp:$(RELAY_PORT)

android: android-reverse ## Run the app on the Android emulator (launches it if ANDROID_EMULATOR=<id>)
	cd app && flutter run -d $(ANDROID_DEVICE)

reset-ios: ## Uninstall the app from the iOS Simulator (deletes its identity and chats); stop `make ios` first
	xcrun simctl uninstall $(IOS_DEVICE) $(APP_ID)

reset-android: ## Uninstall the app from the Android emulator (same); stop `make android` first
	"$(ADB)" -s $(ANDROID_DEVICE) uninstall $(APP_ID)

# --- scripted end-to-end tests (no screenshots, no tapping) ------------------
# The real app on a scratch simulator/emulator (never the phones you chat on:
# the test clears the app's data on its device) against a temp server, with the
# admin's device on the Mac (app/integration_test/run.sh). SCENARIO is chat or
# images. Nothing else needs to be running; the temp server takes a free port.
SCENARIO ?= images

test-ios: ## Scripted app test on a scratch iOS simulator "Vyajena Test"; SCENARIO=chat|images
	app/integration_test/run.sh "$$(deploy/local/devices.sh ios)" $(SCENARIO)

test-android: ## Scripted app test on a scratch emulator (AVD vyajena_test); SCENARIO=chat|images
	app/integration_test/run.sh "$$(deploy/local/devices.sh android)" $(SCENARIO)

test-bridge: ## Rust end-to-end picture tests through the bridge, no device
	cargo test -p bridge --test images
