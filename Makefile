SRC = $(wildcard src/*.c)
OUT = lineit

# Every modules/**/x.c is built to build/modules/**/x.so, loaded at runtime.
# modules/ only has sources.
MODULES_SRC = $(shell find modules -name '*.c')
MODULES = $(patsubst modules/%.c,build/modules/%.so,$(MODULES_SRC))
# Headers shared by modules (compare/compare.h, math/arith.h, logic/logic.h...):
# any change rebuilds them
MODULES_H = $(shell find modules -name '*.h')
# Subgraphs: modules/**/x.lua are canvases used as modules. Linked, not
# copied: saving one in the editor reloads it without make.
GRAPHS = $(patsubst modules/%,build/modules/%,$(shell find modules -name '*.lua'))

# conf.h needs Lua 5.1 or LuaJIT (both implement the Lua 5.1 API).
# Override with: make LUA=luajit
LUA ?= lua5.1
LUA_CFLAGS = $(shell pkg-config --cflags $(LUA) 2>/dev/null)
LUA_LIBS = $(shell pkg-config --libs $(LUA) 2>/dev/null || echo '-l$(LUA) -lm')

# Hardened by default: buffer overflow checks (_FORTIFY_SOURCE, which needs
# optimization, so `make debug` and the AddressSanitizer tests leave it out),
# stack canaries, stack clash protection, a position independent editor and
# read-only relocations. Modules and the runtime get the same.
HARDEN_CFLAGS := -U_FORTIFY_SOURCE -D_FORTIFY_SOURCE=3 -fstack-protector-strong -fstack-clash-protection
HARDEN_LDFLAGS := -pie -Wl,-z,relro,-z,now
CFLAGS ?= -O2 -g $(HARDEN_CFLAGS)
LDFLAGS ?= $(HARDEN_LDFLAGS)
# `make WERROR=1` (CI) makes every warning an error. Not format-truncation:
# snprintf cutting paths and messages to the buffer is what it's there for,
# and gcc warns about every one that could, however big the buffer.
WARN = -Wall -Wextra -Wformat=2 -Wshadow -Wno-format-truncation $(if $(WERROR),-Werror)

# raylib and the libraries in thirdparty/ are -isystem: their warnings are
# theirs, not lineit's
CPPFLAGS += -isystem ./raylib-6.0_linux_amd64/include
CPPFLAGS += $(addprefix -isystem ./thirdparty/,flag.h conf.h cum.h bt.h)
CPPFLAGS += $(LUA_CFLAGS)

# Runtime checks for `make debug` and `make test-asan`: memory errors, leaks
# and undefined behavior
SAN_FLAGS = -fsanitize=address,undefined -fno-sanitize-recover=all -fno-omit-frame-pointer
DEBUG_FLAGS = -O0 -g $(SAN_FLAGS)

# Rebuild everything when the flags change (after `make debug`, or WERROR)
FLAGS := $(CC) $(WARN) $(CFLAGS) $(CPPFLAGS) $(LDFLAGS)
ifneq ($(FLAGS),$(file < build/.flags))
$(shell mkdir -p build)
$(file > build/.flags,$(FLAGS))
endif

DEPS += ./raylib-6.0_linux_amd64/lib/libraylib.a -lX11 -lm
DEPS += $(LUA_LIBS)
DEPS += $(shell pkg-config --libs fontconfig)
DEPS += -ldl
DEPS += $(shell pkg-config --libs zlib)
# libcrypto (OpenSSL): the SHA-256 of the files in packages
DEPS += $(shell pkg-config --libs libcrypto)

all: $(OUT) $(MODULES) $(GRAPHS) $(RT)

# -rdynamic exports the li_* functions (src/lineit.h) to the modules
$(OUT): $(SRC) src/lineit.h src/value.h build/.flags
	$(CC) $(WARN) $(CFLAGS) -fPIE $(CPPFLAGS) $(SRC) $(LDFLAGS) $(DEPS) -rdynamic -o $@

# The same, without optimizations and with the sanitizers, while working on it
debug:
	$(MAKE) CFLAGS='$(DEBUG_FLAGS)' LDFLAGS='$(SAN_FLAGS)'

build/modules/%.lua: modules/%.lua
	@mkdir -p $(dir $@)
	ln -sfr $< $@

build/modules/%.so: modules/%.c src/lineit.h $(MODULES_H) build/.flags
	@mkdir -p $(dir $@)
	$(CC) $(WARN) $(CFLAGS) -I./src -fPIC -shared $< $(LDFLAGS:-pie=) -o $@

# What the programs Compile makes are built with (see Compile in
# src/main.c): build/rt/liblineit-rt.a, and the headers they and their
# modules include in build/rt/include.
RT = build/rt/liblineit-rt.a
RT_H = src/lineit.h src/value.h src/rt/rt.h thirdparty/bt.h/bt.h thirdparty/cum.h/cum.h

$(RT): src/value.c src/rt/rt.c $(RT_H) build/.flags
	@mkdir -p build/rt/include
	cp $(RT_H) build/rt/include/
	$(CC) $(WARN) $(CFLAGS) -Ibuild/rt/include -c src/value.c -o build/rt/value.o
	$(CC) $(WARN) $(CFLAGS) -Ibuild/rt/include -c src/rt/rt.c -o build/rt/rt.o
	$(AR) rcs $@ build/rt/value.o build/rt/rt.o

# examples/*.lua, see tools/gen_examples.c. It includes src/main.c, so it
# links every other source file but that one.
GEN_EXAMPLES = build/gen_examples

examples: $(GEN_EXAMPLES) $(MODULES)
	./$(GEN_EXAMPLES)

$(GEN_EXAMPLES): tools/gen_examples.c $(SRC) src/lineit.h
	@mkdir -p $(dir $@)
	$(CC) $(WARN) $(CFLAGS) $(CPPFLAGS) $< $(filter-out src/main.c,$(SRC)) $(DEPS) -rdynamic -o $@

# Modules packaged with tools/lineit-pack (.li). Only the example, used by
# tests/package.lua.
PACKAGES = build/modules/string/reverse.li

build/modules/string/reverse.li: $(wildcard examples/package/* examples/package/src/*) tools/lineit-pack src/lineit.h
	tools/lineit-pack -o $@ examples/package/Makefile

# Runs every canvas in examples/ and tests/ and compares it with its .out,
# see tools/run_tests.c. `make test-update` rewrites the .out files.
RUN_TESTS = build/run_tests
TEST_EXTRACT = build/test_extract
TESTS = $(wildcard examples/*.lua tests/*.lua)

test: $(RUN_TESTS) $(TEST_EXTRACT) $(MODULES) $(GRAPHS) $(PACKAGES) $(RT)
	./$(RUN_TESTS) $(TESTS)
	./$(RUN_TESTS) -c $(TESTS)
	./$(TEST_EXTRACT)

# The same, with the editor built with AddressSanitizer and UBSan: any
# memory error, leak or undefined behavior fails it

test-asan: build/run_tests_asan build/test_extract_asan $(MODULES) $(GRAPHS) $(PACKAGES) $(RT)
	./build/run_tests_asan $(TESTS)
	./build/run_tests_asan -c $(TESTS)
	./build/test_extract_asan

build/%_asan: tools/%.c $(SRC) src/lineit.h
	@mkdir -p $(dir $@)
	$(CC) $(WARN) -O1 -g $(SAN_FLAGS) $(CPPFLAGS) $< $(filter-out src/main.c,$(SRC)) $(DEPS) -rdynamic -o $@

test-update: $(RUN_TESTS) $(MODULES) $(GRAPHS) $(PACKAGES)
	./$(RUN_TESTS) -u $(TESTS)

# Make subgraph on example canvases, see tools/test_extract.c
$(TEST_EXTRACT): tools/test_extract.c $(SRC) src/lineit.h
	@mkdir -p $(dir $@)
	$(CC) $(WARN) $(CFLAGS) $(CPPFLAGS) $< $(filter-out src/main.c,$(SRC)) $(DEPS) -rdynamic -o $@

$(RUN_TESTS): tools/run_tests.c $(SRC) src/lineit.h
	@mkdir -p $(dir $@)
	$(CC) $(WARN) $(CFLAGS) $(CPPFLAGS) $< $(filter-out src/main.c,$(SRC)) $(DEPS) -rdynamic -o $@

# clang's static analyzer: it follows every path through the code, also the
# ones no test runs, looking for leaks, NULL dereferences, use after free...
# Any finding fails, also in the code of thirdparty/ that lineit calls.
ANALYZE_CC ?= clang
ANALYZE_FLAGS = --analyze --analyzer-output text -Xanalyzer -analyzer-werror
ANALYZE_SRC = $(SRC) src/rt/rt.c $(MODULES_SRC)

analyze:
	@for f in $(ANALYZE_SRC); do \
		echo "analyze $$f"; \
		$(ANALYZE_CC) $(ANALYZE_FLAGS) $(CPPFLAGS) -Isrc $$f -o /dev/null || exit 1; \
	done

# clang-format with .clang-format, on the C of the editor, the runtime, the
# modules and the tools (not thirdparty/ nor raylib). CI checks with
# clang-format $(CLANG_FORMAT_VERSION) (pipx install
# clang-format==$(CLANG_FORMAT_VERSION)): other versions may format differently.
CLANG_FORMAT ?= clang-format
CLANG_FORMAT_VERSION := 23.1.1
FORMAT_SRC = $(shell git ls-files 'src/*.[ch]' 'modules/*.[ch]' 'tools/*.c' 'examples/*.[ch]')

format:
	$(CLANG_FORMAT) -i $(FORMAT_SRC)

check-format:
	$(CLANG_FORMAT) --dry-run -Werror $(FORMAT_SRC)

# docs/*.typ to PDF, next to them (needs typst). Not in git: the docs
# workflow builds them and puts them in the releases.
docs:
	@find docs -name '*.typ' | while read -r f; do echo "typst compile $$f"; typst compile "$$f" || exit 1; done

# The version: LI_VERSION in src/lineit.h, the only one (see Versions in
# docs/internals.typ). `make version` prints it.
VERSION := $(shell sed -n 's/^\#define LI_VERSION "\(.*\)"/\1/p' src/lineit.h)

version:
	@echo $(VERSION)

# make install: to ~/.local (/usr/local as root), or PREFIX, under DESTDIR.
# lineit finds what comes with it from where its binary is (see Paths in
# src/main.c), so the folders below are fixed, relative to PREFIX:
#
#   bin/lineit, bin/lineit-pack
#   lib/lineit/modules            the modules, subgraphs, types and packages
#   lib/lineit/rt                 the runtime of Compile, and its headers
#   share/lineit/modules          the C of the modules, for Compile
#   share/lineit/config.lua       every setting, commented: copy it to
#                                 ~/.config/lineit/config.lua to change them
#   share/lineit/examples         the example canvases
#   share/applications, share/icons
#
# Your own modules, compiled programs and the package cache go to
# ~/.local/share/lineit and ~/.cache/lineit, not here.
PREFIX ?= $(if $(filter 0,$(shell id -u)),/usr/local,$(HOME)/.local)
INSTALL_DIR = $(DESTDIR)$(PREFIX)

install: all
	install -d $(INSTALL_DIR)/bin $(INSTALL_DIR)/lib/lineit/rt/include \
		$(INSTALL_DIR)/share/lineit/examples $(INSTALL_DIR)/share/applications \
		$(INSTALL_DIR)/share/icons/hicolor/scalable/apps
	install -m755 $(OUT) $(INSTALL_DIR)/bin/lineit
	install -m755 tools/lineit-pack $(INSTALL_DIR)/bin/lineit-pack
	rm -rf $(INSTALL_DIR)/lib/lineit/modules $(INSTALL_DIR)/share/lineit/modules
	cp -RL build/modules $(INSTALL_DIR)/lib/lineit/modules
	# What make test leaves there: not lineit's modules
	rm -rf $(INSTALL_DIR)/lib/lineit/modules/test-extract $(PACKAGES:build/modules/%=$(INSTALL_DIR)/lib/lineit/modules/%)
	install -m644 $(RT) $(INSTALL_DIR)/lib/lineit/rt/
	install -m644 build/rt/include/* $(INSTALL_DIR)/lib/lineit/rt/include/
	cp -R modules $(INSTALL_DIR)/share/lineit/modules
	install -m644 config.lua $(INSTALL_DIR)/share/lineit/config.lua
	install -m644 examples/*.lua $(INSTALL_DIR)/share/lineit/examples/
	install -m644 assets/lineit.desktop $(INSTALL_DIR)/share/applications/lineit.desktop
	install -m644 assets/lineit.svg $(INSTALL_DIR)/share/icons/hicolor/scalable/apps/lineit.svg

uninstall:
	rm -f $(INSTALL_DIR)/bin/lineit $(INSTALL_DIR)/bin/lineit-pack
	rm -rf $(INSTALL_DIR)/lib/lineit $(INSTALL_DIR)/share/lineit
	rm -f $(INSTALL_DIR)/share/applications/lineit.desktop \
		$(INSTALL_DIR)/share/icons/hicolor/scalable/apps/lineit.svg

# lineit-VERSION.tar.gz: the source, with the submodules, as git has it. It
# is reproducible: no dates, owners or umask in it (every file rw-r--r--,
# or rwxr-xr-x if it's executable or a dir), so built again from the same
# commit it's the same bytes.
SOURCE_DATE_EPOCH ?= $(shell git log -1 --format=%ct 2>/dev/null || date +%s)
TAR := tar --sort=name --mtime=@$(SOURCE_DATE_EPOCH) --owner=0 --group=0 --numeric-owner --mode=a=rX,u+w --format=gnu
DIST = lineit-$(VERSION)

dist:
	rm -rf build/dist && mkdir -p build/dist/$(DIST)
	git ls-files --recurse-submodules | while IFS= read -r f; do [ ! -e "$$f" ] || echo "$$f"; done | \
		tar -cf - -T - | tar -xf - -C build/dist/$(DIST)
	$(TAR) -cf - -C build/dist $(DIST) | gzip -9n >$(DIST).tar.gz

# lineit-x86_64.AppImage: make install into an AppDir (PREFIX=/usr), with
# every shared library lineit needs (but glibc, X11 and GL, see linuxdeploy's
# excludelist). It finds its modules and runtime inside, like make install
# (see Paths in src/main.c). x86_64 only: that's the raylib in the repo.
# Compile still needs a C compiler and make on the machine.
ARCH := $(shell uname -m)
APPDIR := build/AppDir
APPIMAGE := lineit-$(ARCH).AppImage
# Downloaded once, and checked against these before every use: they end up
# in what's released. The AppImage runtime is pinned too, or appimagetool
# would download its latest one. The tools are AppImages too: run them
# without FUSE (CI has none).
DL := build/downloads
export APPIMAGE_EXTRACT_AND_RUN = 1
LINUXDEPLOY_VERSION := 1-alpha-20251107-1
LINUXDEPLOY_SHA256 := c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
LINUXDEPLOY := $(DL)/linuxdeploy-$(LINUXDEPLOY_VERSION)-x86_64.AppImage
APPIMAGETOOL_VERSION := 1.9.1
APPIMAGETOOL_SHA256 := ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
APPIMAGETOOL := $(DL)/appimagetool-$(APPIMAGETOOL_VERSION)-x86_64.AppImage
RUNTIME_VERSION := 20251108
RUNTIME_SHA256 := 2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
RUNTIME := $(DL)/runtime-$(RUNTIME_VERSION)-x86_64
# linuxdeploy's own strip is too old for the libraries of newer distros
STRIP ?= strip

# $(call check,FILE,SHA256): stop, and remove FILE, unless it has that checksum
check = @echo '$(2)  $(1)' | sha256sum -c --quiet || { rm -f $(1); echo "$(1): wrong checksum, removed it" >&2; exit 1; }

appimage: $(APPIMAGE)

$(APPIMAGE): all $(LINUXDEPLOY) $(APPIMAGETOOL) $(RUNTIME)
	@[ "$(ARCH)" = x86_64 ] || { echo "appimage: only x86_64, the raylib in the repo" >&2; exit 1; }
	$(call check,$(LINUXDEPLOY),$(LINUXDEPLOY_SHA256))
	$(call check,$(APPIMAGETOOL),$(APPIMAGETOOL_SHA256))
	$(call check,$(RUNTIME),$(RUNTIME_SHA256))
	chmod +x $(LINUXDEPLOY) $(APPIMAGETOOL)
	rm -rf $(APPDIR)
	$(MAKE) --no-print-directory install DESTDIR=$(APPDIR) PREFIX=/usr
	NO_STRIP=1 $(LINUXDEPLOY) --appdir $(APPDIR) --executable $(APPDIR)/usr/bin/lineit \
		--desktop-file assets/lineit.desktop --icon-file assets/lineit.svg
	find $(APPDIR)/usr/lib -maxdepth 1 -name '*.so*' -type f -exec $(STRIP) --strip-unneeded {} +
	ARCH=x86_64 $(APPIMAGETOOL) --no-appstream --runtime-file $(RUNTIME) $(APPDIR) $@

$(LINUXDEPLOY):
	@mkdir -p $(@D)
	curl -fsSL -o $@ https://github.com/linuxdeploy/linuxdeploy/releases/download/$(LINUXDEPLOY_VERSION)/linuxdeploy-x86_64.AppImage

$(APPIMAGETOOL):
	@mkdir -p $(@D)
	curl -fsSL -o $@ https://github.com/AppImage/appimagetool/releases/download/$(APPIMAGETOOL_VERSION)/appimagetool-x86_64.AppImage

$(RUNTIME):
	@mkdir -p $(@D)
	curl -fsSL -o $@ https://github.com/AppImage/type2-runtime/releases/download/$(RUNTIME_VERSION)/runtime-x86_64

# Run the tests before every `git push` (scripts/hooks/pre-push)
hooks:
	git config core.hooksPath scripts/hooks

clean:
	rm -f $(OUT) lineit-*.tar.gz lineit-*.AppImage
	rm -rf build
	find docs -name '*.pdf' -delete

.PHONY: all analyze check-format clean debug dist docs examples format hooks install test test-update test-asan
.PHONY: appimage uninstall version
