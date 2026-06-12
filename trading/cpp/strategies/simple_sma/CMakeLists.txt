cmake_minimum_required(VERSION 3.16)
project(simple_sma_strategy LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(simple_sma main.cpp)
target_include_directories(simple_sma PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/../../sdk")

if(MSVC)
  target_compile_options(simple_sma PRIVATE /W4)
else()
  target_compile_options(simple_sma PRIVATE -Wall -Wextra -Wpedantic)
endif()
