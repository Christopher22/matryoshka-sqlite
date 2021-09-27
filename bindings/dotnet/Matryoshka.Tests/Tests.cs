using System;
using System.IO;
using Xunit;

namespace Matryoshka.Tests
{
    public class UnitTests {
        private class TemporaryDirectory : IDisposable {
            private string path;

            public TemporaryDirectory() {
                this.path = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
                Directory.CreateDirectory(this.path);
            }

            ~TemporaryDirectory() {
                CleanUp();
            }

            public string GetFile(string file_name) {
                return Path.Combine(this.path, file_name);
            }

            public string GetFile(string file_name, byte[] data) {
                var path = GetFile(file_name);
                System.IO.File.WriteAllBytes(path, data);
                return path;
            }

            protected void CleanUp() {
                if (this.path != null) {
                    Directory.Delete(this.path, true);
                    this.path = null;
                }
            }

            public void Dispose() {
                CleanUp();
                GC.SuppressFinalize(this);
            }
        }

        [Theory]
        [InlineData("folder/file", new byte[] { }, -1)]
        [InlineData("folder/file", new byte[] { }, 0)]
        [InlineData("folder/file", new byte[] { }, 3)]
        [InlineData("folder/file", new byte[] { 42, 32, 44 }, -1)]
        [InlineData("folder/file", new byte[] { 42, 32, 44 }, 0)]
        [InlineData("folder/file", new byte[] { 42, 32, 44 }, 3)]
        [InlineData("folder/file", new byte[] { 42, 32, 44 }, 4)]
        public void TestIO(string inner_path, byte[] data, int chunk_size) {
            var file_system = new FileSystem(":memory:");

            // Try to load a non-existing file
            Assert.Throws<MatryoshkaException>(() => {
                file_system.Open(inner_path);
            });

            using (var tmp_dir = new TemporaryDirectory()) {
                var input_file = tmp_dir.GetFile("input_file", data);
                var output_file = tmp_dir.GetFile("output_file");

                // Push file
                var file = file_system.Push(inner_path, input_file, chunk_size);
                Assert.Equal(file.Size, data.Length);

                // It is forbidden to override included files
                Assert.Throws<MatryoshkaException>(() => {
                    file_system.Push(inner_path, input_file, chunk_size);
                });

                // Load file and verify content
                var loaded_file = file_system.Open(inner_path);
                loaded_file.Pull(output_file);
                Assert.Equal(data, System.IO.File.ReadAllBytes(output_file));

                // Check delete
                Assert.True(file.Delete());
                Assert.Throws<MatryoshkaException>(() => {
                    file_system.Open(inner_path);
                });
            }
        }

        [Fact]
        public void TestFind() {
            var file_system = new FileSystem(":memory:");

            using (var tmp_dir = new TemporaryDirectory()) {
                var input_file_1 = tmp_dir.GetFile("input_file_1", new byte[] { 42, 32, 44 });
                var input_file_2 = tmp_dir.GetFile("input_file_2", new byte[] { 45, 46, 47 });
                var input_file_3 = tmp_dir.GetFile("input_file_3", new byte[] { 48, 49, 50 });

                // Push files
                file_system.Push("folder1/file1", input_file_1, -1);
                file_system.Push("folder1/file2", input_file_2, -1);
                file_system.Push("folder2/file1", input_file_3, -1);

                // Check find
                Assert.Equal(3, file_system.Find("*").Count);
                Assert.Equal(2, file_system.Find("folder?/file1").Count);
                Assert.Equal(2, file_system.Find("*/file1").Count);
                Assert.Equal(1, file_system.Find("folder2/*").Count);
            }
        }
    }
}
