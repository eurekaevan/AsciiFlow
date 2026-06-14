using System;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;

namespace AsciiFlow.Core.Video;

/// <summary>
/// FFmpeg 动态库路径自动检测器
/// 支持项目目录（开发阶段）和应用目录（发布阶段）
/// </summary>
public static class FFmpegPathResolver
{
    /// <summary>
    /// 解析 FFmpeg 动态库所在目录
    /// </summary>
    /// <param name="userOverride">用户通过参数或环境变量指定的路径（优先级最高）</param>
    /// <returns>FFmpeg 动态库目录路径；未找到时返回 null</returns>
    public static string? Resolve(string? userOverride = null)
    {
        // 按优先级查找（从高到低）
        var candidates = new[]
        {
            userOverride,
            Environment.GetEnvironmentVariable("FFMPEG_ROOT"),
            FindInAppDirectory(),
            FindInProjectRoot(),
            FindInCurrentDirectory(),
        };

        foreach (var path in candidates)
        {
            if (IsValidFFmpegFolder(path))
                return path;
        }

        return null;
    }

    /// <summary>
    /// 应用目录下的 ffmpeg/{platform}/ 子目录
    /// （适用于 dotnet publish 后的发布场景）
    /// </summary>
    private static string? FindInAppDirectory()
    {
        string appDir = AppContext.BaseDirectory;
        return GetPlatformSpecificPath(appDir);
    }

    /// <summary>
    /// 项目根目录下的 ffmpeg/{platform}/（适用于 dotnet run 开发场景）
    /// </summary>
    private static string? FindInProjectRoot()
    {
        string? projectRoot = FindProjectRootFromAppDir();
        if (projectRoot == null) return null;
        return GetPlatformSpecificPath(projectRoot);
    }

    /// <summary>
    /// 当前工作目录（适用于从项目根目录 `dotnet run`）
    /// </summary>
    private static string? FindInCurrentDirectory()
    {
        try
        {
            return GetPlatformSpecificPath(Directory.GetCurrentDirectory());
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// 根据当前操作系统获取子文件夹路径
    /// </summary>
    private static string GetPlatformSpecificPath(string baseDir)
    {
        string subDir = RuntimeInformation.IsOSPlatform(OSPlatform.Windows)
            ? "windows"
            : RuntimeInformation.IsOSPlatform(OSPlatform.OSX)
                ? "macos"
                : "linux";

        return Path.Combine(baseDir, "ffmpeg", subDir);
    }

    /// <summary>
    /// 向上查找项目根目录（包含 .git 或 .sln 的目录），最多向上 5 层
    /// </summary>
    private static string? FindProjectRootFromAppDir()
    {
        try
        {
            var dir = new DirectoryInfo(AppContext.BaseDirectory);

            // 最多向上查找 5 层（覆盖 bin/Debug/net10.0 → 项目根目录）
            for (int i = 0; i < 5 && dir != null; i++, dir = dir.Parent)
            {
                bool hasGit = Directory.Exists(Path.Combine(dir.FullName, ".git"));
                bool hasSln = Directory.GetFiles(dir.FullName, "*.sln", SearchOption.TopDirectoryOnly).Length > 0;
                if (hasGit || hasSln) return dir.FullName;
            }
        }
        catch
        {
            // 权限不足等错误，忽略
        }
        return null;
    }

    /// <summary>
    /// 验证候选目录是否真的是 FFmpeg 库所在目录
    /// </summary>
    private static bool IsValidFFmpegFolder(string? path)
    {
        if (string.IsNullOrEmpty(path) || !Directory.Exists(path))
            return false;

        try
        {
            // 根据操作系统检查关键 DLL/SO 文件
            string[] markers = RuntimeInformation.IsOSPlatform(OSPlatform.Windows)
                ? new[] { "avcodec-*.dll", "avformat-*.dll", "avutil-*.dll" }
                : new[] { "libavcodec.so*", "libavformat.so*", "libavutil.so*" };

            foreach (string pattern in markers)
            {
                if (Directory.GetFiles(path, pattern).Length > 0)
                    return true;
            }
        }
        catch
        {
            // 忽略 IO 错误
        }

        return false;
    }

    /// <summary>
    /// 获取详细的错误帮助信息
    /// </summary>
    public static string GetHelpMessage()
    {
        string platform = RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "Windows"
                        : RuntimeInformation.IsOSPlatform(OSPlatform.OSX) ? "macOS"
                        : "Linux";

        return $@"
┌────────────────────────────────────────────────────────────────┐
│  ❌ FFmpeg 动态库未找到                                         │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  当前平台: {platform,-55} │
│  应用目录: {AppContext.BaseDirectory,-55} │
│                                                                │
│  期望目录结构:                                                  │
│    AsciiFlow/                                                  │
│    └── ffmpeg/                                                 │
│        └── {(RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "windows/" : RuntimeInformation.IsOSPlatform(OSPlatform.OSX) ? "macos/  " : "linux/   ")}                                         │
│            ├─ {(RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "avcodec-61.dll" : "libavcodec.so.*",-50)} │
│            ├─ {(RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "avformat-61.dll" : "libavformat.so.*",-50)} │
│            └─ ...                                              │
│                                                                │
│  解决方式:                                                      │
│    1. 将 FFmpeg 库文件放入项目根目录下的 ffmpeg/{(RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "windows" : "linux")}/ 文件夹 │
│    2. 设置环境变量 FFMPEG_ROOT=/path/to/ffmpeg                 │
│    3. 将 FFmpeg 库所在目录加入系统 PATH                        │
│                                                                │
│  获取 FFmpeg:                                                  │
│    Windows: https://www.gyan.dev/ffmpeg/builds/                │
│    Linux:   sudo apt install libavcodec-dev libavformat-dev ... │
│    macOS:   brew install ffmpeg                                │
│                                                                │
└────────────────────────────────────────────────────────────────┘
";
    }
}