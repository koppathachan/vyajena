# Local development shortcuts: a Pi, a stand-in relay, the iOS Simulator and an
# Android emulator. `make help` lists them. Nothing here touches the real Pi or
# the GCP relay (deploy/pi and deploy/relay do that, on purpose and by hand).
#
# Typical session, one terminal each:
#   make pi        (or: make pi-relayed, and make relay)
#   make ios
#   make android
# The first phone joins with the bootstrap invite the Pi prints on its very
# first start (README: "Run the app locally").

SHELL := /bin/bash
.DEFAULT_GOAL := help

PI_PORT       ?= 8443
RELAY_PORT    ?= 9443
ANDROID_HOME  ?= $(HOME)/Library/Android/sdk
ADB           ?= $(shell command -v adb || echo $(ANDROID_HOME)/platform-tools/adb)
# `make emulators` lists ids; `make devices` lists running ones.
ANDROID_EMULATOR ?=
ANDROID_DEVICE   ?= emulator-5554
# Defaults to the booted simulator; pass IOS_DEVICE=<id> to pick one.
IOS_DEVICE    ?= $(shell xcrun simctl list devices booted 2>/dev/null | grep -Eo '[0-9A-F]{8}-([0-9A-F]{4}-){3}[0-9A-F]{12}' | head -1)

# Git-ignored with the rest of server/.pi-local/.
RELAYED_DIR := server/.pi-local/relayed

.PHONY: help pi pi-relayed relay ios ios-sim android android-emulator android-reverse \
        pi-reset pi-relayed-reset reset-ios reset-android rust-targets devices emulators

help: ## List the targets
	@grep -hE '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | sed -E 's/^([a-z-]+):.*## (.*)/  make \1\t\2/' | expand -t 24

# --- the Pi and the relay ----------------------------------------------------

pi: ## Local Pi on 127.0.0.1:8443 (server/local.toml); the invite prints on first start only
	cargo run -p server -- --config server/local.toml

# Rewritten on every run, so PI_PORT and RELAY_PORT overrides take effect.
.PHONY: $(RELAYED_DIR)/pi.toml
$(RELAYED_DIR)/pi.toml:
	mkdir -p $(RELAYED_DIR)
	printf '%s\n' \
	  'server_id = "pi"' \
	  'admin_username = "akhil"' \
	  'listen_address = "127.0.0.1:$(PI_PORT)"' \
	  'public_address = "127.0.0.1:$(RELAY_PORT)"' \
	  'database_path = "pi.sqlite"' \
	  'key_dir = "keys"' > $@

pi-relayed: $(RELAYED_DIR)/pi.toml ## Local Pi whose invites name the relay port, own data; needs `make relay`
	cargo run -p server -- --config $<

relay: ## Stand-in relay: TCP passthrough 127.0.0.1:9443 -> the Pi (not rathole)
	python3 deploy/local/relay.py $(RELAY_PORT) $(PI_PORT)

pi-reset: ## Delete the local Pi's data (new keys, new bootstrap invite); stop it first
	rm -rf server/.pi-local/pi.sqlite* server/.pi-local/keys

pi-relayed-reset: ## Same for the relayed Pi; stop it first
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

android-reverse: android-emulator ## adb reverse the Pi and relay ports (emulator 127.0.0.1 -> the Mac)
	"$(ADB)" -s $(ANDROID_DEVICE) reverse tcp:$(PI_PORT) tcp:$(PI_PORT)
	"$(ADB)" -s $(ANDROID_DEVICE) reverse tcp:$(RELAY_PORT) tcp:$(RELAY_PORT)

android: android-reverse ## Run the app on the Android emulator (launches it if ANDROID_EMULATOR=<id>)
	cd app && flutter run -d $(ANDROID_DEVICE)

reset-ios: ## Uninstall the app from the iOS Simulator (deletes its identity and chats); stop `make ios` first
	xcrun simctl uninstall $(IOS_DEVICE) $(APP_ID)

reset-android: ## Uninstall the app from the Android emulator (same); stop `make android` first
	"$(ADB)" -s $(ANDROID_DEVICE) uninstall $(APP_ID)
