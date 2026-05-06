const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Create the root module that imports all kernels
    const root_module = b.createModule(.{
        .root_source_file = b.path("src/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const lib = b.addLibrary(.{
        .name = "beamz",
        .root_module = root_module,
        .linkage = .static,
        .pic = true,  // Position Independent Code for linking into PIE executables
    });

    b.installArtifact(lib);

    // Run tests for the root module
    const main_tests = b.addTest(.{
        .root_module = root_module,
    });

    const test_step = b.step("test", "Run tests");
    test_step.dependOn(&main_tests.step);
}
