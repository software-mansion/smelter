cmake_minimum_required(VERSION 3.15)

set(PROJECT_NAME rustium_cef_wrapper)

project(${PROJECT_NAME})

set(_CEF_ROOT_EXPLICIT 1)
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} ${CEF_ROOT}/cmake)

include("cef_macros")
include("cef_variables")

include_directories(${CEF_ROOT}/libcef_dll)
add_subdirectory(${CEF_ROOT} libcef_dll)

install(TARGETS libcef_dll_wrapper DESTINATION .)
