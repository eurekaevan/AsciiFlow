import os
import time
from concurrent.futures import ProcessPoolExecutor
from PIL import Image

# 帧文件夹路径
frames_dir = 'cxk_ascii_frames'  # 改成你的路径
frame_files = [os.path.join(frames_dir, f) for f in os.listdir(frames_dir) if f.lower().endswith(('.jpg', '.png'))]

# 简单模拟的帧处理函数（比如转换为灰度）
def process_frame(file_path):
    try:
        img = Image.open(file_path).convert('L')  # 转灰度图模拟处理
        _ = img.resize((100, 100))  # 模拟一点计算量
    except Exception as e:
        print(f"Error processing {file_path}: {e}")

# 测试不同线程数的耗时
def benchmark_processing(worker_counts):
    results = {}
    for workers in worker_counts:
        start = time.perf_counter()
        with ProcessPoolExecutor(max_workers=workers) as executor:
            list(executor.map(process_frame, frame_files))
        elapsed = time.perf_counter() - start
        results[workers] = elapsed
        print(f"max_workers = {workers}，耗时：{elapsed:.2f} 秒")
    return results

# 你可以修改这里的线程数列表
test_workers = [1, 4, 8, 12, 16, 20, 22]

# 跑起来
if __name__ == "__main__":
    print(f"共 {len(frame_files)} 张帧图，开始测试...\n")
    benchmark_processing(test_workers)
