CFLAGS ?= -Wall -Wextra

.PHONY: all clean

all: hello

hello: main.c include/greeting.h
	@$(CC) $(CFLAGS) main.c -o $@

clean:
	$(RM) hello
