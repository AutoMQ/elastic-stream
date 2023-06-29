DBG_MAKEFILE ?=
ifeq ($(DBG_MAKEFILE),1)
    $(warning ***** starting Makefile for goal(s) "$(MAKECMDGOALS)")
    $(warning ***** $(shell date))
else
    # If we're not debugging the Makefile, don't echo recipes.
    MAKEFLAGS += -s
endif

# The binaries to build (just the basename).
BINS ?= range-server

# The platforms we support.  In theory this can be used for Windows platforms,
# too, but they require specific base images, which we do not have.
ALL_PLATFORMS ?= linux/amd64 linux/arm64

# The "FROM" part of the Dockerfile.  This should be a manifest-list which
# supports all of the platforms listed in ALL_PLATFORMS.
BASE_IMAGE ?= gcr.io/distroless/static

# Where to push the docker images.
REGISTRY ?= docker.io/elasticstream

# This version-strategy uses git tags to set the version string
VERSION ?= $(shell git describe --tags --always --dirty)

# Set this to 1 to build in release mode.
RELEASE ?=

###
### These variables should not need tweaking.
###

# We don't need make's built-in rules.
MAKEFLAGS += --no-builtin-rules
# Be pedantic about undefined variables.
MAKEFLAGS += --warn-undefined-variables
.SUFFIXES:

# It's necessary to set this because some environments don't link sh -> bash.
SHELL := /usr/bin/env bash -o errexit -o pipefail -o nounset

OS ?= unknown
ifeq ($(OS),Windows_NT)
  HOST_OS := windows
else
  HOST_OS := $(shell uname -s | tr A-Z a-z)
endif
ifeq ($(shell uname -m),x86_64)
  HOST_ARCH := amd64
else
  HOST_ARCH := $(shell uname -m)
endif

# Used internally.  Users should pass RUST_OS and/or RUST_ARCH.
OS := $(if $(RUST_OS),$(RUST_OS),$(HOST_OS))
ARCH := $(if $(RUST_ARCH),$(RUST_ARCH),$(HOST_ARCH))

ifeq ($(OS)_$(ARCH),linux_amd64)
  TARGET := x86_64-unknown-linux-gnu
else ifeq ($(OS)_$(ARCH),linux_arm64)
  TARGET := aarch64-unknown-linux-gnu
else
  $(error Unsupported OS/ARCH $(OS)/$(ARCH))
endif

BUILD_TYPE :=
ifeq ($(RELEASE),1)
  BUILD_TYPE := --release
endif

BIN_EXTENSION :=
ifeq ($(OS), windows)
  BIN_EXTENSION := .exe
endif

TAG := $(VERSION)__$(OS)_$(ARCH)

# This is used in docker buildx commands
BUILDX_NAME := $(shell basename $$(pwd))

# If you want to build all binaries, see the 'all-build' rule.
# If you want to build all containers, see the 'all-container' rule.
# If you want to build AND push all containers, see the 'all-push' rule.
all: # @HELP builds binaries for one platform ($OS/$ARCH)
all: build

# For the following OS/ARCH expansions, we transform OS/ARCH into OS_ARCH
# because make pattern rules don't match with embedded '/' characters.

build-%:
	$(MAKE) build                         \
	    --no-print-directory              \
	    RUST_OS=$(firstword $(subst _, ,$*)) \
	    RUST_ARCH=$(lastword $(subst _, ,$*))

container-%:
	$(MAKE) container                     \
	    --no-print-directory              \
	    RUST_OS=$(firstword $(subst _, ,$*)) \
	    RUST_ARCH=$(lastword $(subst _, ,$*))

push-%:
	$(MAKE) push                          \
	    --no-print-directory              \
	    RUST_OS=$(firstword $(subst _, ,$*)) \
	    RUST_ARCH=$(lastword $(subst _, ,$*))

all-build: # @HELP builds binaries for all platforms
all-build: $(addprefix build-, $(subst /,_, $(ALL_PLATFORMS)))

all-container: # @HELP builds containers for all platforms
all-container: $(addprefix container-, $(subst /,_, $(ALL_PLATFORMS)))

all-push: # @HELP pushes containers for all platforms to the defined registry
all-push: $(addprefix push-, $(subst /,_, $(ALL_PLATFORMS)))

build: # @HELP builds the binary for the current platform
build: target/$(TARGET)/$(BUILD_TYPE)/$(BINS)$(BIN_EXTENSION)

.PHONY: target/$(TARGET)/$(BUILD_TYPE)/$(BINS)$(BIN_EXTENSION)
target/$(TARGET)/$(BUILD_TYPE)/$(BINS)$(BIN_EXTENSION):
	cross build --target $(TARGET) $(BUILD_TYPE)
	echo -ne "binary: target/$(TARGET)/$(BUILD_TYPE)/$(BINS)$(BIN_EXTENSION)"
	echo

container:
	echo "TODO"

help: # @HELP prints this message
help:
	echo "VARIABLES:"
	echo "  BINS = $(BINS)"
	echo "  OS = $(OS)"
	echo "  ARCH = $(ARCH)"
	echo "  TARGET = $(TARGET)"
	echo "  BUILD_TYPE = $(BUILD_TYPE)"
	echo "  REGISTRY = $(REGISTRY)"
	echo
	echo "TARGETS:"
	grep -E '^.*: *# *@HELP' $(MAKEFILE_LIST)     \
	    | awk '                                   \
	        BEGIN {FS = ": *# *@HELP"};           \
	        { printf "  %-30s %s\n", $$1, $$2 };  \
	    '
