# SPDX-License-Identifier: Apache-2.0
# Exercise real make dependency timestamps, separate compilation and incremental relinking.
CC = gcc
all: made
made: main.o helper.o
	$(CC) main.o helper.o -o made
main.o: main.c
	$(CC) -c main.c -o main.o
helper.o: helper.c
	$(CC) -c helper.c -o helper.o
