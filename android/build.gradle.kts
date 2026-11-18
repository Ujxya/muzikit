plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.compose.compiler) apply false
}

// Derleme çıktısı android/ dışında: muzikit/.build/android/<modül> (android/ klasörü olduğu gibi yüklenebilsin).
allprojects {
    layout.buildDirectory = rootProject.layout.projectDirectory.dir("../.build/android/${project.name}")
}
