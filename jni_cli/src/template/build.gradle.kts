/*
 * This file was generated by `jni_cli`
 *
 */

plugins {
    // Apply the org.jetbrains.kotlin.jvm Plugin to add support for Kotlin.
    id("org.jetbrains.kotlin.jvm") version "1.9.0"
    id("maven-publish")

    // Apply the java-library plugin for API and implementation separation.
    `java-library`
}

repositories {
    // Use Maven Central for resolving dependencies.
    mavenCentral()
    maven {
        url = uri("https://bio.informatik.uni-jena.de/repository/libs-release-oss/")
    }
}

dependencies {
    // This dependency is exported to consumers, that is to say found on their compile classpath.
    implementation("cz.adamh:native-utils:1.0")
    api("org.apache.commons:commons-math3:3.6.1")
    // This dependency is used internally, and not exposed to consumers on their own compile
    // classpath.
    implementation("com.google.guava:guava:32.1.1-jre")
}

testing {
    suites {
        // Configure the built-in test suite
        val test by
                getting(JvmTestSuite::class) {
                    // Use Kotlin Test test framework
                    useKotlinTest("1.9.0")
                }
    }
}

// Apply a specific Java toolchain to ease working on different environments.
java { toolchain { languageVersion.set(JavaLanguageVersion.of(19)) } }

publishing {
    publications {
        create<MavenPublication>("maven") {
            groupId = "{{group_id}}"
            artifactId = "{{package_name}}"
            version = "1.1"

            from(components["java"])
        }
    }
}
