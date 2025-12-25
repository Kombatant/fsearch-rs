cmake_minimum_required(VERSION 3.16)
project(fsearch_qt_client LANGUAGES CXX)

find_package(Qt6 COMPONENTS Widgets REQUIRED)

add_executable(fsearch_qt_client main.cpp)
target_link_libraries(fsearch_qt_client PRIVATE Qt6::Widgets)

# Adjust include and link paths as needed for the Rust staticlib output
option(FSEARCH_CORE_LIB "Path to fsearch-core static library" "")
option(FSEARCH_CORE_INCLUDE_DIR "Path to fsearch-core include directory" "${CMAKE_CURRENT_SOURCE_DIR}/../fsearch-core/include")

target_include_directories(fsearch_qt_client PRIVATE ${FSEARCH_CORE_INCLUDE_DIR})

# Auto-detect the Rust staticlib in the workspace: ../target/release/libfsearch_core.a
if(NOT FSEARCH_CORE_LIB)
	set(_default_lib "${CMAKE_CURRENT_SOURCE_DIR}/../target/release/libfsearch_core.a")
	if(EXISTS ${_default_lib})
		set(FSEARCH_CORE_LIB ${_default_lib})
		message(STATUS "Auto-detected fsearch-core library: ${FSEARCH_CORE_LIB}")
	endif()
endif()

if(FSEARCH_CORE_LIB)
	# Create imported static library target and link it
	add_library(fsearch_core_static UNKNOWN IMPORTED)
	set_target_properties(fsearch_core_static PROPERTIES IMPORTED_LOCATION "${FSEARCH_CORE_LIB}")
	# Link fsearch core static lib. If the Rust staticlib depends on libpcre2, also link it.
	find_library(PCRE2_LIB pcre2-8)
	if(PCRE2_LIB)
		target_link_libraries(fsearch_qt_client PRIVATE fsearch_core_static ${PCRE2_LIB})
	else()
		target_link_libraries(fsearch_qt_client PRIVATE fsearch_core_static)
	endif()
else()
	message(WARNING "FSEARCH_CORE_LIB not provided and auto-detection failed; pass -DFSEARCH_CORE_LIB=/path/to/libfsearch_core.a to link the Rust library.")
endif()
