import os
from PIL import Image, ImageDraw


def create_warning_icon(base_path, output_png, output_ico):
    if not os.path.exists(base_path):
        print(f"Base icon not found at {base_path}")
        return

    # Open base image
    img = Image.open(base_path).convert("RGBA")

    # Calculate circle size and position (top right corner)
    width, height = img.size
    radius = int(width * 0.15)  # 15% of width

    # Create a draw object
    draw = ImageDraw.Draw(img)

    # Draw red circle in top right
    center_x = width - radius - int(width * 0.05)
    center_y = radius + int(height * 0.05)

    # Draw outer white stroke for contrast
    draw.ellipse(
        (
            center_x - radius - 2,
            center_y - radius - 2,
            center_x + radius + 2,
            center_y + radius + 2,
        ),
        fill=(255, 255, 255, 255),
    )

    # Draw red circle
    draw.ellipse(
        (center_x - radius, center_y - radius, center_x + radius, center_y + radius),
        fill=(255, 0, 0, 255),
    )

    # Save PNG
    img.save(output_png, "PNG")
    print(f"Saved {output_png}")

    # Save ICO (multiple sizes for better Windows support)
    img.save(output_ico, "ICO", sizes=[(16, 16), (32, 32), (64, 64), (128, 128)])
    print(f"Saved {output_ico}")


if __name__ == "__main__":
    icon_dir = "icons"
    base_icon = os.path.join(icon_dir, "icon.png")
    out_png = os.path.join(icon_dir, "icon-warning.png")
    out_ico = os.path.join(icon_dir, "icon-warning.ico")

    create_warning_icon(base_icon, out_png, out_ico)
