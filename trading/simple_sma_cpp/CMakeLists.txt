cmake_minimum_required(VERSION 3.16)
project(simple_sma_strategy LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(simple_sma_cpp main.cpp)
target_include_directories(simple_sma_cpp PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/../_sdk/cpp")

if(MSVC)
  target_compile_options(simple_sma_cpp PRIVATE /W4)
else()
  target_compile_options(simple_sma_cpp PRIVATE -Wall -Wextra -Wpedantic)
endif()
