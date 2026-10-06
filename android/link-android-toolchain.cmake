# Ergänzungen für den Android-Build des Ableton-Link-SDK (rusty_link).
#
# Das SDK kennt keine eigene Android-Plattform; Android ist für Link Linux
# (clock_gettime, getifaddrs, pthread – alles vorhanden). Beim Cross-Build
# fehlt deshalb LINK_PLATFORM_LINUX, wodurch platform::Random/IoContext/Clock
# fehlen.
#
# Bionic (Android) hat kein `htonll` (nur glibc), das Link in
# NetworkByteStreamSerializable.hpp benutzt. Der Shim daneben stellt es bereit.
#
# Die eigentliche Android-Konfiguration (System, Compiler, Flags) setzt die
# cmake-Kiste über -D; ein Erweitern von CMAKE_CXX_FLAGS hier wird davon
# überschrieben. Deshalb hängt diese Datei nur eine Projekt-Include-Datei ein,
# die *nach* project() läuft und die beiden Ergänzungen sauber setzt.

set(CMAKE_PROJECT_INCLUDE "${CMAKE_CURRENT_LIST_DIR}/link_shim/android-link.cmake" CACHE FILEPATH "")
