import cv2
import numpy as np
import os
import re
from PIL import Image, ImageDraw, ImageFont
from concurrent.futures import ThreadPoolExecutor, ProcessPoolExecutor
from tqdm import tqdm
import shutil

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

def save_image(args):
    img, path = args
    cv2.imwrite(path, cv2.cvtColor(img, cv2.COLOR_RGB2BGR), [int(cv2.IMWRITE_JPEG_QUALITY), 95])
    return path

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

def convert_video_to_ascii_images(video_path, output_dir="ascii_frames", width=160,
                                  process_workers=4, thread_workers=4, ext="jpg"):
    os.makedirs(output_dir, exist_ok=True)

    # 获取视频总帧数
    cap = cv2.VideoCapture(video_path)
    total_frames = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
    cap.release()

    print(f"视频总帧数约为：{total_frames}，开始边读边处理...")

    saved_paths = []

    with ProcessPoolExecutor(max_workers=process_workers, initializer=init_worker) as process_executor, \
         ThreadPoolExecutor(max_workers=thread_workers) as thread_executor:

        # 进度条包装 frame_generator
        frames = frame_generator(video_path)
        frames_gen = ((frame, width) for _, frame in tqdm(frames, total=total_frames, desc="读取帧"))

        # 多进程处理转换，边处理边更新进度条
        processed_images_iter = process_executor.map(frame_to_image, frames_gen, chunksize=4)

        futures = []
        for i, img in enumerate(tqdm(processed_images_iter, total=total_frames, desc="转换 ASCII 帧")):
            path = os.path.join(output_dir, f"frame_{i:06d}.{ext}")
            futures.append(thread_executor.submit(save_image, (img, path)))

        # 多线程保存图片，并显示保存进度条
        for future in tqdm(futures, desc="保存图片", total=len(futures)):
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

    print("正在将帧合成为视频...")

    def load_frame(path):
        return cv2.imread(path)

    with ThreadPoolExecutor(max_workers=8) as executor:
        for frame in tqdm(executor.map(load_frame, image_files), total=len(image_files)):
            out.write(frame)

    out.release()

def clean_up(directory):
    shutil.rmtree(directory, ignore_errors=True)

if __name__ == "__main__":
    video_path = "cxk.mp4"
    base_name = re.search(r"(.+?)\.", os.path.basename(video_path)).group(1)
    output_dir = base_name + "_ascii_frames"
    output_video_path = base_name + "_ascii_output.mp4"

    print("\u2728 Step 1\uff1a转换为 ASCII 图像帧")
    frame_paths = convert_video_to_ascii_images(
        video_path,
        output_dir=output_dir,
        width=200,
        process_workers=12,
        thread_workers=12,
        ext="jpg"
    )

    print("\ud83c\udfae Step 2\uff1a合成视频")
    generate_video_from_images(frame_paths, output_video_path, fps=25)

    # 是否清理中间帧（释放硬盘空间）
    clean_up(output_dir)

    print("\u2705 完成！视频输出路径：", output_video_path)
