# A module packaged with tools/lineit-pack, which runs `make lineit-package`
# here (see tools/lineit-pack --howto):
#
#     tools/lineit-pack -o build/modules/string/reverse.li examples/package/Makefile
#
# It builds for this machine, and for macOS too if MACOS_CC is set to a
# compiler for it (on a mac: MACOS_CC=cc; on Linux, osxcross: o64-clang).

LINEIT_INCLUDE ?= ../../src
PLATFORM := $(shell uname -s | sed 's/Linux/linux/;s/Darwin/macos/')-$(shell uname -m | sed 's/aarch64/arm64/')
EXT := $(if $(findstring macos,$(PLATFORM)),dylib,so)
FLAGS := $(if $(findstring macos,$(PLATFORM)),-dynamiclib -undefined dynamic_lookup,-fPIC -shared)

lineit-package:
	@test -n "$(PACKAGE_DIR)" || { echo "run it with tools/lineit-pack"; exit 1; }
	mkdir -p $(PACKAGE_DIR)/lib $(PACKAGE_DIR)/src
	$(CC) -O2 $(FLAGS) -I$(LINEIT_INCLUDE) reverse.c -o $(PACKAGE_DIR)/lib/$(PLATFORM).$(EXT)
ifneq ($(MACOS_CC),)
	$(MACOS_CC) -O2 -dynamiclib -undefined dynamic_lookup -I$(LINEIT_INCLUDE) reverse.c -o $(PACKAGE_DIR)/lib/macos-arm64.dylib
endif
	cp manifest.lua LICENSE $(PACKAGE_DIR)/
	cp reverse.c Makefile $(PACKAGE_DIR)/src/

.PHONY: lineit-package
