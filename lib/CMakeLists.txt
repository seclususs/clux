cmake_minimum_required(VERSION 3.21)

project(clux LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

add_library(clux STATIC base.cpp)

target_include_directories(clux PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")

target_compile_options(
  clux PRIVATE -fno-exceptions -fno-rtti -fno-threadsafe-statics
               -fvisibility=hidden -nostdinc++)
