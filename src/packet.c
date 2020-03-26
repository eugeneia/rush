#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>

#define PAYLOAD_SIZE 1024*10

struct Packet {
  unsigned short length;
  unsigned char data[PAYLOAD_SIZE];
};

#define FREELIST_SIZE 1000

struct Freelist {
  unsigned long nfree;
  struct Packet *list[FREELIST_SIZE];
};

static struct Freelist fl;

void init () {
  while (fl.nfree < FREELIST_SIZE) {
    struct Packet *p = malloc(sizeof(struct Packet));
    memset(p, 0, sizeof(struct Packet));
    fl.list[fl.nfree] = p;
    fl.nfree++;
  }
}

struct Packet *allocate () {
  assert(fl.nfree > 0 && "Packet freelist underflow.");
  fl.nfree--;
  return fl.list[fl.nfree];
}

void free_ (struct Packet *p) {
  assert(fl.nfree < FREELIST_SIZE && "Packet freelist overflow.");
  p->length = 0;
  fl.list[fl.nfree] = p;
  fl.nfree++;
}

void main () {
  init();
  struct Packet *p = allocate();
  printf("Allocated packet of size: %d\n", p->length);
  p->length = 1;
  p->data[0] = 42;
  printf("Can mutate packet: p->length = %d, p->data[0] = %d\n",
         p->length, p->data[0]);
  free_(p);
  printf("Freed p (ownership ends)\n");
}
