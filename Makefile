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

CFLAGS += -I./raylib-6.0_linux_amd64/include/
CFLAGS += -I./thirdparty/flag.h
CFLAGS += -I./thirdparty/conf.h
CFLAGS += -I./thirdparty/cum.h
CFLAGS += -I./thirdparty/bt.h
CFLAGS += $(LUA_CFLAGS)

DEPS += ./raylib-6.0_linux_amd64/lib/libraylib.a -lX11 -lm
DEPS += $(LUA_LIBS)
DEPS += $(shell pkg-config --libs fontconfig)
DEPS += -ldl
DEPS += $(shell pkg-config --libs zlib)

all: $(OUT) $(MODULES) $(GRAPHS)

# -rdynamic exports the li_* functions (src/lineit.h) to the modules
$(OUT): $(SRC) src/lineit.h
	$(CC) $(CFLAGS) $(SRC) $(DEPS) -rdynamic -o $@

build/modules/%.lua: modules/%.lua
	@mkdir -p $(dir $@)
	ln -sfr $< $@

build/modules/%.so: modules/%.c src/lineit.h $(MODULES_H)
	@mkdir -p $(dir $@)
	$(CC) -I./src -fPIC -shared $< -o $@

# examples/*.lua, see tools/gen_examples.c. It includes src/main.c, so it
# links every other source file but that one.
GEN_EXAMPLES = build/gen_examples

examples: $(GEN_EXAMPLES) $(MODULES)
	./$(GEN_EXAMPLES)

$(GEN_EXAMPLES): tools/gen_examples.c $(SRC) src/lineit.h
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) $< $(filter-out src/main.c,$(SRC)) $(DEPS) -rdynamic -o $@

# Modules packaged with tools/lineit-pack (.li). Only the example, used by
# tests/package.lua.
PACKAGES = build/modules/string/reverse.li

build/modules/string/reverse.li: $(wildcard examples/package/*) tools/lineit-pack src/lineit.h
	tools/lineit-pack -o $@ examples/package/Makefile

# Runs every canvas in examples/ and tests/ and compares it with its .out,
# see tools/run_tests.c. `make test-update` rewrites the .out files.
RUN_TESTS = build/run_tests
TEST_EXTRACT = build/test_extract
TESTS = $(wildcard examples/*.lua tests/*.lua)

test: $(RUN_TESTS) $(TEST_EXTRACT) $(MODULES) $(GRAPHS) $(PACKAGES)
	./$(RUN_TESTS) $(TESTS)
	./$(TEST_EXTRACT)

# The same, with the editor built with AddressSanitizer and UBSan: any
# memory error, leak or undefined behavior fails it
ASAN_FLAGS = -g -fsanitize=address,undefined -fno-sanitize-recover=all -fno-omit-frame-pointer

test-asan: build/run_tests_asan build/test_extract_asan $(MODULES) $(GRAPHS) $(PACKAGES)
	./build/run_tests_asan $(TESTS)
	./build/test_extract_asan

build/%_asan: tools/%.c $(SRC) src/lineit.h
	@mkdir -p $(dir $@)
	$(CC) $(ASAN_FLAGS) $(CFLAGS) $< $(filter-out src/main.c,$(SRC)) $(DEPS) -rdynamic -o $@

test-update: $(RUN_TESTS) $(MODULES) $(GRAPHS) $(PACKAGES)
	./$(RUN_TESTS) -u $(TESTS)

# Make subgraph on example canvases, see tools/test_extract.c
$(TEST_EXTRACT): tools/test_extract.c $(SRC) src/lineit.h
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) $< $(filter-out src/main.c,$(SRC)) $(DEPS) -rdynamic -o $@

$(RUN_TESTS): tools/run_tests.c $(SRC) src/lineit.h
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) $< $(filter-out src/main.c,$(SRC)) $(DEPS) -rdynamic -o $@

clean:
	rm -f $(OUT)
	rm -rf build

.PHONY: all clean examples test test-update test-asan
