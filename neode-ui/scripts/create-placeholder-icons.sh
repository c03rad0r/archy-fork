#!/bin/bash
# Create simple placeholder icons using ImageMagick or fallback to SVG

cd "$(dirname "$0")/.."
ICON_DIR="public/assets/img/app-icons"

# Create endurain placeholder
if command -v convert &> /dev/null; then
  convert -size 512x512 xc:none -fill "rgba(100,150,255,200)" -draw "circle 256,256 256,100" -pointsize 200 -fill white -gravity center -annotate +0+0 "E" "$ICON_DIR/endurain.png" 2>/dev/null && echo "✅ Created endurain.png"
elif command -v magick &> /dev/null; then
  magick -size 512x512 xc:none -fill "rgba(100,150,255,200)" -draw "circle 256,256 256,100" -pointsize 200 -fill white -gravity center -annotate +0+0 "E" "$ICON_DIR/endurain.png" 2>/dev/null && echo "✅ Created endurain.png"
else
  # Fallback: Create simple SVG
  cat > "$ICON_DIR/endurain.svg" << 'SVGEOF'
<svg width="512" height="512" xmlns="http://www.w3.org/2000/svg">
  <circle cx="256" cy="256" r="156" fill="rgba(100,150,255,200)"/>
  <text x="256" y="256" font-size="200" fill="white" text-anchor="middle" dominant-baseline="central" font-weight="bold">E</text>
</svg>
SVGEOF
  echo "✅ Created endurain.svg"
fi

# Create morphos-server placeholder
if command -v convert &> /dev/null; then
  convert -size 512x512 xc:none -fill "rgba(150,100,255,200)" -draw "rectangle 100,100 412,412" -pointsize 200 -fill white -gravity center -annotate +0+0 "M" "$ICON_DIR/morphos-server.png" 2>/dev/null && echo "✅ Created morphos-server.png"
elif command -v magick &> /dev/null; then
  magick -size 512x512 xc:none -fill "rgba(150,100,255,200)" -draw "rectangle 100,100 412,412" -pointsize 200 -fill white -gravity center -annotate +0+0 "M" "$ICON_DIR/morphos-server.png" 2>/dev/null && echo "✅ Created morphos-server.png"
else
  # Fallback: Create simple SVG
  cat > "$ICON_DIR/morphos-server.svg" << 'SVGEOF'
<svg width="512" height="512" xmlns="http://www.w3.org/2000/svg">
  <rect x="100" y="100" width="312" height="312" rx="40" fill="rgba(150,100,255,200)"/>
  <text x="256" y="256" font-size="200" fill="white" text-anchor="middle" dominant-baseline="central" font-weight="bold">M</text>
</svg>
SVGEOF
  echo "✅ Created morphos-server.svg"
fi

