cmake_minimum_required(VERSION 3.16)
project(fcitx5-hfastkey VERSION 0.1.0 LANGUAGES CXX)

find_package(Fcitx5Core 5.1 REQUIRED)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(HFASTKEY_CORE_LIB "${CMAKE_CURRENT_SOURCE_DIR}/../target/release/libhfastkey_core.a"
    CACHE FILEPATH "Thư viện tĩnh lõi Rust (cargo build --release)")

add_library(hfastkey MODULE hfastkey.cpp)
target_link_libraries(hfastkey PRIVATE Fcitx5::Core "${HFASTKEY_CORE_LIB}" pthread dl m)
# Không để lộ ký hiệu của thư viện chuẩn Rust ra ngoài.
target_link_options(hfastkey PRIVATE "-Wl,--exclude-libs,ALL" "-Wl,--no-undefined")
set_target_properties(hfastkey PROPERTIES PREFIX "lib")

install(TARGETS hfastkey DESTINATION "${FCITX_INSTALL_ADDONDIR}")
install(FILES hfastkey-addon.conf RENAME hfastkey.conf DESTINATION "${FCITX_INSTALL_PKGDATADIR}/addon")
install(FILES hfastkey-telex.conf hfastkey-vni.conf DESTINATION "${FCITX_INSTALL_PKGDATADIR}/inputmethod")
