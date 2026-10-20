cmake_minimum_required(VERSION 3.20)
project(bergamot_shim CXX C)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_subdirectory(${BERGAMOT_SRC} bergamot)

# marian-fork sets its ARM/SSE/BLAS defines per directory; code including its headers needs the same ones.
get_directory_property(MARIAN_DEFS DIRECTORY ${BERGAMOT_SRC}/marian-fork COMPILE_DEFINITIONS)
get_directory_property(MARIAN_OPTS DIRECTORY ${BERGAMOT_SRC}/marian-fork COMPILE_OPTIONS)
target_compile_definitions(bergamot-translator-source PUBLIC ${MARIAN_DEFS} USE_SENTENCEPIECE _USE_INTERNAL_STRING_VIEW)
target_compile_options(bergamot-translator-source PUBLIC ${MARIAN_OPTS} -fno-strict-aliasing)

add_library(bergamot_shim STATIC shim.cpp)
target_link_libraries(bergamot_shim PRIVATE bergamot-translator-source)
