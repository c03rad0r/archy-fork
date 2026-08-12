#!/bin/bash

# Video Optimization Script for Web - 1MB Target
# Optimizes video-intro.mp4 to ~1MB for fast web loading

set -e

VIDEO_DIR="public/assets/video"
INPUT_FILE="${VIDEO_DIR}/video-intro.mp4"
OUTPUT_FILE="${VIDEO_DIR}/video-intro-optimized.mp4"
BACKUP_FILE="${VIDEO_DIR}/video-intro-backup-$(date +%Y%m%d-%H%M%S).mp4"

echo "🎬 Video Optimization Script - 1MB Target"
echo "=========================================="
echo ""

# Check if FFmpeg is installed
if ! command -v ffmpeg &> /dev/null; then
    echo "❌ FFmpeg is not installed."
    echo ""
    echo "Install it with:"
    echo "  macOS: brew install ffmpeg"
    echo "  Linux: sudo apt install ffmpeg"
    echo "  Windows: Download from https://ffmpeg.org/download.html"
    exit 1
fi

# Check if input file exists
if [ ! -f "$INPUT_FILE" ]; then
    echo "❌ Input file not found: $INPUT_FILE"
    exit 1
fi

echo "📹 Input file: $INPUT_FILE"
INPUT_SIZE=$(du -h "$INPUT_FILE" | cut -f1)
echo "   Size: $INPUT_SIZE"
echo ""

# Get video info
echo "📊 Analyzing video..."
DURATION=$(ffprobe -v quiet -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 "$INPUT_FILE" 2>/dev/null || echo "unknown")
RESOLUTION=$(ffprobe -v quiet -select_streams v:0 -show_entries stream=width,height -of csv=s=x:p=0 "$INPUT_FILE" 2>/dev/null || echo "unknown")
FPS=$(ffprobe -v quiet -select_streams v:0 -show_entries stream=r_frame_rate -of default=noprint_wrappers=1:nokey=1 "$INPUT_FILE" 2>/dev/null | awk -F'/' '{print $1/$2}' | head -1 || echo "unknown")
echo "   Duration: ${DURATION}s"
echo "   Resolution: ${RESOLUTION}"
echo "   Frame rate: ${FPS}fps"
echo ""

# Create backup
echo "💾 Creating backup..."
cp "$INPUT_FILE" "$BACKUP_FILE"
echo "   Backup saved to: $BACKUP_FILE"
echo ""

# Optimize video for 1MB target
echo "⚙️  Optimizing video for ~1MB target..."
echo "   Resolution: 1280x720 (HD)"
echo "   Frame rate: 30fps"
echo "   CRF: 30 (good quality, smaller file)"
echo "   Audio: 64kbps (background music quality)"
echo ""

ffmpeg -i "$INPUT_FILE" \
  -c:v libx264 \
  -preset slow \
  -crf 30 \
  -profile:v high \
  -level 4.0 \
  -pix_fmt yuv420p \
  -vf "scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2" \
  -r 30 \
  -c:a aac \
  -b:a 64k \
  -ar 44100 \
  -movflags +faststart \
  -threads 0 \
  -y \
  "$OUTPUT_FILE" 2>&1 | grep -E "(Duration|Stream|frame|size|time)" || true

if [ $? -eq 0 ] && [ -f "$OUTPUT_FILE" ]; then
    echo ""
    echo "✅ Optimization complete!"
    echo ""
    
    OUTPUT_SIZE=$(du -h "$OUTPUT_FILE" | cut -f1)
    OUTPUT_BYTES=$(stat -f%z "$OUTPUT_FILE" 2>/dev/null || stat -c%s "$OUTPUT_FILE" 2>/dev/null)
    
    if [ -n "$OUTPUT_BYTES" ]; then
        OUTPUT_MB=$(echo "scale=2; $OUTPUT_BYTES / 1024 / 1024" | bc 2>/dev/null || echo "unknown")
        echo "📊 Results:"
        echo "   Original size: $INPUT_SIZE"
        echo "   Optimized size: $OUTPUT_SIZE (~${OUTPUT_MB}MB)"
        
        # Calculate compression ratio
        ORIGINAL_BYTES=$(stat -f%z "$INPUT_FILE" 2>/dev/null || stat -c%s "$INPUT_FILE" 2>/dev/null)
        if [ -n "$ORIGINAL_BYTES" ] && [ -n "$OUTPUT_BYTES" ]; then
            RATIO=$(echo "scale=1; ($ORIGINAL_BYTES - $OUTPUT_BYTES) * 100 / $ORIGINAL_BYTES" | bc)
            echo "   Size reduction: ${RATIO}%"
            
            # Check if target achieved
            TARGET_BYTES=1048576  # 1MB in bytes
            if [ "$OUTPUT_BYTES" -gt "$TARGET_BYTES" ]; then
                EXCESS_MB=$(echo "scale=2; ($OUTPUT_BYTES - $TARGET_BYTES) / 1024 / 1024" | bc)
                echo ""
                echo "⚠️  File size is ${EXCESS_MB}MB over 1MB target"
                echo "   Current: ~${OUTPUT_MB}MB"
                echo ""
                echo "   Options to reduce further:"
                echo "   - Use CRF 32 (slightly lower quality, smaller file)"
                echo "   - Reduce resolution to 854x480"
                echo "   - Reduce frame rate to 24fps"
            else
                echo ""
                echo "✅ Target achieved! File is under 1MB"
            fi
        fi
    fi
    
    echo ""
    echo "🔄 Replacing original file..."
    mv "$OUTPUT_FILE" "$INPUT_FILE"
    echo "   ✅ Original file replaced with optimized version"
    echo ""
    echo "💡 To restore backup:"
    echo "   mv \"$BACKUP_FILE\" \"$INPUT_FILE\""
else
    echo ""
    echo "❌ Optimization failed. Original file preserved."
    rm -f "$OUTPUT_FILE"
    exit 1
fi

echo ""
echo "✨ Done! Video optimized for web (~1MB target)."

