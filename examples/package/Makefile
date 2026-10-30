# A lineit package. Every package has a Makefile like this one at its root,
# next to src/, and it goes in the package too: lineit runs it on what's in
# the package (see tools/lineit-pack --howto). Its rules get, as variables:
#
#   OUT             where to write what it builds, absolute
#   LINEIT_INCLUDE  folder with lineit.h, for -I
#   CC, CFLAGS      the compiler and flags to build with
#
#   lineit-so       the module as a library, for the editor
#   lineit-o        src/ as one relocatable object (cc -r), for Compile
#   lineit-libs     prints what that object needs to link (-lcurl -lm...),
#                   nothing if it needs nothing
#   lineit-package  what tools/lineit-pack runs here to make the .li:
#
#     tools/lineit-pack -o build/modules/string/reverse.li examples/package/Makefile
#
# It builds for this machine, and for macOS too if MACOS_CC is set to a
# compiler for it (on a mac: MACOS_CC=cc; on Linux, osxcross: o64-clang).

LINEIT_INCLUDE ?= ../../src
CFLAGS ?= -O2
SRC = $(wildcard src/*.c)
# Libraries it uses, if any: -lcurl...
LIBS =

PLATFORM := $(shell uname -s | sed 's/Linux/linux/;s/Darwin/macos/')-$(shell uname -m | sed 's/aarch64/arm64/')
EXT := $(if $(findstring macos,$(PLATFORM)),dylib,so)
SO_FLAGS := $(if $(findstring macos,$(PLATFORM)),-dynamiclib -undefined dynamic_lookup,-fPIC -shared)

lineit-so:
	@test -n "$(OUT)" || { echo "lineit-so: needs OUT"; exit 1; }
	$(CC) $(CFLAGS) $(SO_FLAGS) -I$(LINEIT_INCLUDE) $(SRC) $(LIBS) -o $(OUT)

lineit-o:
	@test -n "$(OUT)" || { echo "lineit-o: needs OUT"; exit 1; }
	$(CC) $(CFLAGS) -r -nostdlib -I$(LINEIT_INCLUDE) $(SRC) -o $(OUT)

lineit-libs:
	@echo $(LIBS)

lineit-package:
	@test -n "$(PACKAGE_DIR)" || { echo "run it with tools/lineit-pack"; exit 1; }
	mkdir -p $(PACKAGE_DIR)/lib
	$(MAKE) lineit-so OUT=$(PACKAGE_DIR)/lib/$(PLATFORM).$(EXT)
ifneq ($(MACOS_CC),)
	$(MAKE) lineit-so CC=$(MACOS_CC) SO_FLAGS="-dynamiclib -undefined dynamic_lookup" OUT=$(PACKAGE_DIR)/lib/macos-arm64.dylib
endif
	cp -r manifest.lua LICENSE Makefile src $(PACKAGE_DIR)/

.PHONY: lineit-so lineit-o lineit-libs lineit-package
