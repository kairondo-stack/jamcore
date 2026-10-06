#pragma once

#include <stdint.h>

// Android/Bionic kennt kein htonll (das ist eine glibc-Erweiterung), das
// Ableton-Link-SDK benutzt es aber ungeschützt in
// link/include/ableton/discovery/NetworkByteStreamSerializable.hpp.
// Die Konvertierung muss zur Byte-Reihenfolge des Ziels passen.
static inline uint64_t htonll(uint64_t value)
{
  static const uint16_t probe = 1;
  if (*(const uint8_t*)&probe == 1)
  {
    // Little Endian: umdrehen.
    return __builtin_bswap64(value);
  }
  return value;
}

static inline uint64_t ntohll(uint64_t value)
{
  return htonll(value);
}
