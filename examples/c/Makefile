.PHONY: all clean

all: build/bell-state build/deutsch-jozsa

clean:
	[ ! -d build ] || rm -r build

CFLAGS_qblaze!=pkg-config --cflags qblaze
LDFLAGS_qblaze!=pkg-config --libs qblaze

build/%: %.c
	@mkdir -p build
	$(CC) $< -o $@ $(CFLAGS_qblaze) $(LDFLAGS_qblaze)
