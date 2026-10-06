# Wird von CMAKE_PROJECT_INCLUDE nach jedem project()-Aufruf eingebunden
# (siehe link-android-toolchain.cmake): Link als Linux-Plattform bauen und den
# htonll-Shim von Bionic nachreichen.

add_compile_definitions(LINK_PLATFORM_LINUX=1)
add_compile_options(-include "${CMAKE_CURRENT_LIST_DIR}/htonll.h")
