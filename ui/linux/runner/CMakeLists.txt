cmake_minimum_required(VERSION 3.19)
project(runner LANGUAGES CXX)

# Heyo's version, from the one place it is written. `heyo-ui --version`
# says it, and Heyo refuses to start an interface built for another Heyo
# (src-tauri/src/shell/flutter.rs).
set(HEYO_CONFIG "${CMAKE_SOURCE_DIR}/../../src-tauri/tauri.conf.json")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${HEYO_CONFIG}")
file(READ "${HEYO_CONFIG}" HEYO_CONFIG_TEXT)
string(JSON HEYO_VERSION GET "${HEYO_CONFIG_TEXT}" version)

# Define the application target. To change its name, change BINARY_NAME in the
# top-level CMakeLists.txt, not the value here, or `flutter run` will no longer
# work.
#
# Any new source files that you add to the application should be added here.
add_executable(${BINARY_NAME}
  "main.cc"
  "heyo_window.cc"
  "${FLUTTER_MANAGED_DIR}/generated_plugin_registrant.cc"
)

# Apply the standard set of build settings. This can be removed for applications
# that need different build settings.
apply_standard_settings(${BINARY_NAME})

add_definitions(-DAPPLICATION_ID="${APPLICATION_ID}")
add_definitions(-DHEYO_VERSION="${HEYO_VERSION}")

# Add dependency libraries. Add any application-specific dependencies here.
target_link_libraries(${BINARY_NAME} PRIVATE flutter)
target_link_libraries(${BINARY_NAME} PRIVATE PkgConfig::GTK)

target_include_directories(${BINARY_NAME} PRIVATE "${CMAKE_SOURCE_DIR}")
