SRC = $(wildcard src/*.c)
OUT = lineit

# Every modules/**/x.c is built to build/modules/**/x.so, loaded at runtime.
# modules/ only has sources.
MODULES_SRC = $(shell find modules -name '*.c')
MODULES = $(patsubst modules/%.c,build/modules/%.so,$(MODULES_SRC))
# Headers shared by modules (compare/compare.h, math/arith.h, logic/logic.h...):
# any change rebuilds them
MODULES_H = $(shell find modules -name '*.h')

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

all: $(OUT) $(MODULES)

# -rdynamic exports the li_* functions (src/lineit.h) to the modules
$(OUT): $(SRC) src/lineit.h
	$(CC) $(CFLAGS) $(SRC) $(DEPS) -rdynamic -o $@

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

clean:
	rm -f $(OUT)
	rm -rf build

.PHONY: all clean examples
