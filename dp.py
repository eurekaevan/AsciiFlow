import os
import subprocess
import cv2
import re
import shutil
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor, as_completed
from PIL import Image, ImageDraw, ImageFont
import numpy as np
from tqdm import tqdm

# ASCII 字符集，从密集到稀疏
ascii_chars = "MW@&%#*/+=-_'\";:,. "
FONT = None
FONT_SIZE = 12
FONT_PATH = "C:/Windows/Fonts/consola.ttf"

def init_worker():
    global FONT
    FONT = ImageFont.truetype(FONT_PATH, FONT_SIZE)

def pixel_to_ascii(r, g, b):
    gray = int(0.299 * r + 0.587 * g + 0.114 * b)
    char = ascii_chars[int(gray / 256 * len(ascii_chars))]
    return char, (r, g, b)

def frame_to_image(frame_width_tuple):
    frame, width = frame_width_tuple
    global FONT
    h, w = frame.shape[:2]
    aspect_ratio = h / w
    new_height = int(aspect_ratio * width)
    resized = cv2.resize(frame, (width, new_height))
    img = Image.new("RGB", (width * FONT_SIZE, new_height * FONT_SIZE), (0, 0, 0))
    draw = ImageDraw.Draw(img)
    for y, row in enumerate(resized):
        for x, bgr in enumerate(row):
            b, g, r = bgr
            char, color = pixel_to_ascii(r, g, b)
            draw.text((x * FONT_SIZE, y * FONT_SIZE), char, fill=color, font=FONT)
    return np.array(img)

def frame_generator(video_path):
    cap = cv2.VideoCapture(video_path)
    idx = 0
    while True:
        ret, frame = cap.read()
        if not ret:
            break
        yield idx, frame
        idx += 1
    cap.release()

def save_image(args):
    img, path = args
    cv2.imwrite(path, cv2.cvtColor(img, cv2.COLOR_RGB2BGR), [int(cv2.IMWRITE_JPEG_QUALITY), 95])
    return path

def convert_video_to_ascii_images(video_path, output_dir="ascii_frames", width=160,
                                 process_workers=4, thread_workers=4, ext="jpg"):
    os.makedirs(output_dir, exist_ok=True)
    cap = cv2.VideoCapture(video_path)
    total_frames = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
    cap.release()

    saved_paths = []
    with ProcessPoolExecutor(max_workers=process_workers, initializer=init_worker) as process_executor, \
         ThreadPoolExecutor(max_workers=thread_workers) as thread_executor:

        frames_gen = ((frame, width) for _, frame in frame_generator(video_path))
        processed_images_iter = process_executor.map(frame_to_image, frames_gen, chunksize=4)

        futures = []
        for i, img in enumerate(tqdm(processed_images_iter, total=total_frames, desc="转换 ASCII 帧")):
            path = os.path.join(output_dir, f"frame_{i:06d}.{ext}")
            futures.append(thread_executor.submit(save_image, (img, path)))

        for future in tqdm(as_completed(futures), total=len(futures), desc="保存图片"):
            saved_paths.append(future.result())

    return saved_paths

def generate_video_from_images(image_files, output_path, fps=30):
    if not image_files:
        print("没有帧可以生成视频")
        return

    image_files.sort()
    sample = cv2.imread(image_files[0])
    height, width, _ = sample.shape
    fourcc = cv2.VideoWriter_fourcc(*'mp4v')
    out = cv2.VideoWriter(output_path, fourcc, fps, (width, height))

    for path in tqdm(image_files, desc="合成视频"):
        frame = cv2.imread(path)
        if frame is not None:
            out.write(frame)

    out.release()
    print("✅ 完成！视频输出路径：", output_path)

def clean_up(directory):
    shutil.rmtree(directory, ignore_errors=True)

def split_video(video_path, segment_time=10, output_prefix="segment"):
    cmd = [
        "ffmpeg", "-i", video_path,
        "-c", "copy", "-map", "0",
        "-segment_time", str(segment_time),
        "-f", "segment",
        f"{output_prefix}_%03d.mp4"
    ]
    subprocess.run(cmd, check=True)

def process_segments(segment_dir, output_dir_base="ascii_outputs", width=300, fps=25):
    os.makedirs(output_dir_base, exist_ok=True)
    output_segments = []
    segment_files = sorted(f for f in os.listdir(segment_dir) if f.endswith(".mp4"))

    for i, seg_file in enumerate(segment_files):
        seg_path = os.path.join(segment_dir, seg_file)
        print(f"\n🔄 处理片段 {seg_file}...")

        frame_dir = os.path.join(output_dir_base, f"segment_{i:03d}_frames")
        ascii_video_path = os.path.join(output_dir_base, f"segment_{i:03d}.mp4")

        frame_paths = convert_video_to_ascii_images(
            seg_path,
            output_dir=frame_dir,
            width=width,
            process_workers=12,
            thread_workers=12,
            ext="jpg"
        )

        generate_video_from_images(frame_paths, ascii_video_path, fps=fps)
        clean_up(frame_dir)
        output_segments.append(ascii_video_path)

    return output_segments

def merge_videos_ffmpeg(video_paths, output_path="final_ascii_video.mp4"):
    list_path = "video_list.txt"
    with open(list_path, "w", encoding="utf-8") as f:
        for path in video_paths:
            f.write(f"file '{os.path.abspath(path)}'\n")
    cmd = [
    "ffmpeg", "-f", "concat", "-safe", "0",
    "-fflags", "+genpts",
    "-i", list_path,
    "-c", "copy",
    output_path
    ]
    subprocess.run(cmd, check=True)
    os.remove(list_path)
    print(f"✅ 合成完成：{output_path}")

if __name__ == "__main__":
    video_path = "cxk.mp4"
    if not os.path.exists(video_path):
        print(f"错误: 找不到视频文件 '{video_path}'")
        exit()

    print("⏳ Step 1：切割原始视频...")
    split_prefix = "segments/seg"
    os.makedirs("segments", exist_ok=True)
    split_video(video_path, segment_time=10, output_prefix=split_prefix)

    print("🧩 Step 2：处理每个视频片段...")
    segment_output_dir = "ascii_outputs"
    ascii_segments = process_segments("segments", output_dir_base=segment_output_dir, width=200, fps=25)

    print("🎬 Step 3：合并所有 ASCII 视频片段...")
    merge_videos_ffmpeg(ascii_segments, output_path="ascii_final_output.mp4")

    print("✅ 所有步骤完成。你可以查看生成的视频：ascii_final_output.mp4")

