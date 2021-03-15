using System;
using System.Collections.Generic;
using System.Security.Permissions;
using System.Runtime.InteropServices;
using System.Runtime.ConstrainedExecution;
using Microsoft.Win32.SafeHandles;

namespace Matryoshka {
    delegate void FindCallback(IntPtr path);

    /// <summary>
    /// Contains the raw c methods
    /// </summary>
    internal unsafe static class Native {
        public unsafe struct FileSystem { };
        public unsafe struct Status { };
        public unsafe struct FileHandle { };

        [DllImport("matryoshka")]
        public static extern FileSystem* Load([MarshalAs(UnmanagedType.LPUTF8Str)] string path, Status** status);

        [DllImport("matryoshka")]
        public static extern void DestroyFileSystem(FileSystem* file_system);

        [DllImport("matryoshka")]
        public static extern void DestroyStatus(Status* status);

        [DllImport("matryoshka")]
        public static extern void DestroyFileHandle(FileHandle* file_handle);

        [DllImport("matryoshka")]
        public static extern IntPtr GetMessage(Status* status);

        [DllImport("matryoshka")]
        public static extern FileHandle* Open(FileSystem* file_system, string path, Status** status);

        [DllImport("matryoshka")]
        public static extern FileHandle* Push(FileSystem* file_system, string inner_path, string path, int chunk_size, Status** status);

        [DllImport("matryoshka")]
        public static extern Status* Pull(FileSystem* file_system, FileHandle* file, string path);

        [DllImport("matryoshka")]
        public static extern int Find(FileSystem* file_system, IntPtr path, [MarshalAs(UnmanagedType.FunctionPtr)]FindCallback callback);

        [DllImport("matryoshka")]
        public static extern int Find(FileSystem* file_system, string path, [MarshalAs(UnmanagedType.FunctionPtr)]FindCallback callback);

        [DllImport("matryoshka")]
        public static extern int GetSize(FileSystem* file_system, FileHandle* file);

        [DllImport("matryoshka")]
        public static extern int Delete(FileSystem* file_system, FileHandle* file);
    }

    /// <summary>
    /// Namespace containing handlers for easy access
    /// </summary>
    namespace handles {

        /// <summary>
        /// Wraps a native file system and ensure delete.
        /// </summary>
        [SecurityPermission(SecurityAction.InheritanceDemand, UnmanagedCode = true)]
        [SecurityPermission(SecurityAction.Demand, UnmanagedCode = true)]
        internal class FileSystemHandle : SafeHandleZeroOrMinusOneIsInvalid {
            public unsafe FileSystemHandle(Native.FileSystem* file_system) : base(true) {
                this.SetHandle(new IntPtr((void*)file_system));
            }

            public unsafe Native.FileSystem* GetHandle() {
                if (this.IsInvalid) {
                    throw new InvalidOperationException("Handle is already closed");
                }
                return (Native.FileSystem*)this.handle.ToPointer();
            }

            [ReliabilityContract(Consistency.WillNotCorruptState, Cer.MayFail)]
            override protected bool ReleaseHandle() {
                unsafe {
                    Native.DestroyFileSystem((Native.FileSystem*)this.handle.ToPointer());
                }
                return true;
            }
        }

        /// <summary>
        /// Wraps a native status and ensure delete.
        /// </summary>
        [SecurityPermission(SecurityAction.InheritanceDemand, UnmanagedCode = true)]
        [SecurityPermission(SecurityAction.Demand, UnmanagedCode = true)]
        internal class StatusHandle : SafeHandleZeroOrMinusOneIsInvalid {
            public unsafe StatusHandle(Native.Status* status) : base(true) {
                this.SetHandle(new IntPtr((void*)status));
            }

            public string GetMessage() {
                if (this.IsInvalid) {
                    return "";
                }

                unsafe {
                    Native.Status* status_pointer = (Native.Status*)this.handle.ToPointer();
                    return Marshal.PtrToStringAnsi(Native.GetMessage(status_pointer));
                }
            }

            [ReliabilityContract(Consistency.WillNotCorruptState, Cer.MayFail)]
            override protected bool ReleaseHandle() {
                unsafe {
                    Native.DestroyStatus((Native.Status*)this.handle.ToPointer());
                }
                return true;
            }
        }

        /// <summary>
        /// Wraps a native file handle and ensure delete.
        /// </summary>
        [SecurityPermission(SecurityAction.InheritanceDemand, UnmanagedCode = true)]
        [SecurityPermission(SecurityAction.Demand, UnmanagedCode = true)]
        internal class FileHandle : SafeHandleZeroOrMinusOneIsInvalid {
            public unsafe FileHandle(Native.FileHandle* status) : base(true) {
                this.SetHandle(new IntPtr((void*)status));
            }

            [ReliabilityContract(Consistency.WillNotCorruptState, Cer.MayFail)]
            override protected bool ReleaseHandle() {
                unsafe {
                    Native.DestroyFileHandle((Native.FileHandle*)this.handle.ToPointer());
                }
                return true;
            }

            public unsafe Native.FileHandle* GetHandle() {
                if (this.IsInvalid) {
                    throw new InvalidOperationException("Handle is already closed");
                }
                return (Native.FileHandle*)this.handle.ToPointer();
            }
        }
    }

    /// <summary>
    /// A exception thrown on operation failure.
    /// </summary>
    public class MatryoshkaException : Exception {
        internal MatryoshkaException(handles.StatusHandle handle) : base(handle.GetMessage()) { }
    }

    /// <summary>
    /// A virtual file system in a Matryoshka file.
    /// </summary>
    public class FileSystem : IDisposable {
        private readonly handles.FileSystemHandle handle_;

        public FileSystem(string path) {
            unsafe {
                Native.Status* status;
                Native.FileSystem* file_system = Native.Load(path, &status);
                if (file_system == null) {
                    using (handles.StatusHandle handle = new handles.StatusHandle(status)) {
                        throw new MatryoshkaException(handle);
                    }
                } else {
                    handle_ = new handles.FileSystemHandle(file_system);
                }
            }
        }

        public void Dispose() {
            Dispose(true);
            GC.SuppressFinalize(this);
        }

        [SecurityPermission(SecurityAction.Demand, UnmanagedCode = true)]
        protected virtual void Dispose(bool disposing) {
            if (handle_ != null && !handle_.IsInvalid) {
                handle_.Dispose();
            }
        }

        public File Open(string path) {
            unsafe {
                Native.Status* status;
                Native.FileSystem* file_system = handle_.GetHandle();
                Native.FileHandle* file = Native.Open(file_system, path, &status);
                if (file == null) {
                    using (handles.StatusHandle handle = new handles.StatusHandle(status)) {
                        throw new MatryoshkaException(handle);
                    }
                } else {
                    return new File(this, file, path);
                }
            }
        }

        public File Push(string inner_path, string path, int chunk_size = -1) {
            unsafe {
                Native.Status* status;
                Native.FileSystem* file_system = handle_.GetHandle();
                Native.FileHandle* file = Native.Push(file_system, inner_path, path, chunk_size, &status);
                if (file == null) {
                    using (handles.StatusHandle handle = new handles.StatusHandle(status)) {
                        throw new MatryoshkaException(handle);
                    }
                } else {
                    return new File(this, file, path);
                }
            }
        }

        public List<string> Find(string path = null) {
            List<string> files = new List<string>();
            unsafe {
                Native.FileSystem* file_system = handle_.GetHandle();
                if (path == null) {
                    Native.Find(file_system, IntPtr.Zero, x => files.Add(Marshal.PtrToStringAnsi(x)));
                } else {
                    Native.Find(file_system, path, x => files.Add(Marshal.PtrToStringAnsi(x)));
                }
            }
            return files;
        }

        internal unsafe Native.FileSystem* GetHandle() {
            return handle_.GetHandle();
        }
    }

    /// <summary>
    /// A file as part of a virtual file system in Matryoshka.
    /// </summary>
    public class File {
        private readonly handles.FileHandle handle_;
        private readonly FileSystem parent_;
        private readonly string path_;

        internal unsafe File(FileSystem parent, Native.FileHandle* handle, string path) {
            parent_ = parent;
            handle_ = new handles.FileHandle(handle);
            path_ = path;
        }

        public void Pull(string file) {
            unsafe {
                Native.Status* status = Native.Pull(parent_.GetHandle(), handle_.GetHandle(), file);
                if (status != null) {
                    using (handles.StatusHandle handle = new handles.StatusHandle(status)) {
                        throw new MatryoshkaException(handle);
                    }
                }
            }
        }

        public bool Delete() {
            unsafe {
                return Native.Delete(parent_.GetHandle(), handle_.GetHandle()) == 1;
            }
        }

        public int Size {
            get {
                unsafe {
                    return Native.GetSize(parent_.GetHandle(), handle_.GetHandle());
                }
            }
        }

        public override string ToString() {
            return path_;
        }
    }
}