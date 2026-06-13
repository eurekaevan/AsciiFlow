using System.Buffers;

namespace AsciiFlow.Core.Utils;

/// <summary>
/// 缓冲区池，用于减少 GC 压力
/// 提供预分配缓冲区的租借和归还机制
/// </summary>
public static class BufferPool
{
    private static readonly ArrayPool<byte> _bytePool = ArrayPool<byte>.Shared;
    
    /// <summary>
    /// 从池中租借字节数组
    /// </summary>
    /// <param name="size">所需大小</param>
    /// <returns>字节数组（使用完后必须归还）</returns>
    public static byte[] Rent(int size)
    {
        return _bytePool.Rent(size);
    }
    
    /// <summary>
    /// 归还字节数组到池
    /// </summary>
    /// <param name="array">要归还的数组</param>
    public static void Return(byte[] array)
    {
        _bytePool.Return(array, clearArray: false);
    }
    
    /// <summary>
    /// 租借并初始化（清零）的字节数组
    /// </summary>
    public static byte[] RentAndInitialize(int size)
    {
        var array = Rent(size);
        Array.Clear(array, 0, size);
        return array;
    }
}