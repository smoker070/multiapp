// make-assets.swift — generates AppIcon master PNG + DMG background PNG
// Run: swift make-assets.swift   (build.sh does this automatically when assets are missing)
import AppKit

func savePNG(_ image: NSImage, to path: String, pixelsWide: Int, pixelsHigh: Int) {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixelsWide, pixelsHigh: pixelsHigh,
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: pixelsWide, height: pixelsHigh)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    image.draw(in: NSRect(x: 0, y: 0, width: pixelsWide, height: pixelsHigh))
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

// ---------------------------------------------------------------- app icon (1024 master)
func makeIcon() -> NSImage {
    let S: CGFloat = 1024
    let img = NSImage(size: NSSize(width: S, height: S))
    img.lockFocus()

    // Apple-style margin: artwork squircle ~824pt centered
    let inset: CGFloat = 100
    let rect = NSRect(x: inset, y: inset, width: S - 2*inset, height: S - 2*inset)
    let squircle = NSBezierPath(roundedRect: rect, xRadius: 185, yRadius: 185)

    // black → dark pink (Ongli Ozish HQ palette)
    let grad = NSGradient(colors: [
        NSColor(calibratedRed: 0.88, green: 0.16, blue: 0.45, alpha: 1),   // dark pink top
        NSColor(calibratedRed: 0.16, green: 0.03, blue: 0.09, alpha: 1)    // near-black bottom
    ])!
    grad.draw(in: squircle, angle: -90)

    // subtle inner highlight
    let hi = NSBezierPath(roundedRect: rect.insetBy(dx: 6, dy: 6), xRadius: 180, yRadius: 180)
    NSColor.white.withAlphaComponent(0.10).setStroke()
    hi.lineWidth = 10
    hi.stroke()

    // glyph: stacked layers (same motif as the menu-bar icon)
    let cfg = NSImage.SymbolConfiguration(pointSize: 430, weight: .medium)
    if let sym = NSImage(systemSymbolName: "square.3.layers.3d", accessibilityDescription: nil)?
        .withSymbolConfiguration(cfg) {
        let tinted = NSImage(size: sym.size)
        tinted.lockFocus()
        sym.draw(at: .zero, from: .zero, operation: .sourceOver, fraction: 1)
        NSColor.white.set()
        NSRect(origin: .zero, size: sym.size).fill(using: .sourceAtop)
        tinted.unlockFocus()
        let g = tinted.size
        // shadow
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.35)
        shadow.shadowBlurRadius = 24
        shadow.shadowOffset = NSSize(width: 0, height: -12)
        shadow.set()
        tinted.draw(in: NSRect(x: (S-g.width*1.0)/2, y: (S-g.height*1.0)/2 + 8,
                               width: g.width, height: g.height))
    }
    img.unlockFocus()
    return img
}

// ---------------------------------------------------------------- dmg background (560x340 pt @2x)
func makeDMGBackground() -> NSImage {
    let W: CGFloat = 1120, H: CGFloat = 680   // @2x of 560x340
    let img = NSImage(size: NSSize(width: W, height: H))
    img.lockFocus()

    NSGradient(colors: [
        NSColor(calibratedRed: 0.055, green: 0.045, blue: 0.055, alpha: 1),   // near-black
        NSColor(calibratedRed: 0.02, green: 0.015, blue: 0.02, alpha: 1)
    ])!.draw(in: NSRect(x: 0, y: 0, width: W, height: H), angle: -90)

    // faint dark-pink glow top-right
    NSColor(calibratedRed: 0.88, green: 0.16, blue: 0.45, alpha: 0.10).setFill()
    NSBezierPath(ovalIn: NSRect(x: W-420, y: H-420, width: 560, height: 560)).fill()

    func text(_ s: String, size: CGFloat, weight: NSFont.Weight, color: NSColor, at p: NSPoint, centered: Bool = false) {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: size, weight: weight),
            .foregroundColor: color
        ]
        let str = NSAttributedString(string: s, attributes: attrs)
        var pt = p
        if centered { pt.x -= str.size().width / 2 }
        str.draw(at: pt)
    }

    // title + subtitle (coordinates in @2x pixels; Finder origin bottom-left)
    text("Multiapp", size: 64, weight: .bold, color: .white, at: NSPoint(x: W/2, y: H-140), centered: true)
    text("one app · many profiles", size: 30, weight: .regular,
         color: NSColor(calibratedRed: 0.88, green: 0.45, blue: 0.62, alpha: 0.75),
         at: NSPoint(x: W/2, y: H-196), centered: true)

    // arrow between icon slots (app at x=140pt, Applications at x=420pt → @2x: 280 / 840; icons sit ~y=150pt → @2x 300)
    let y: CGFloat = 330
    let arrow = NSBezierPath()
    arrow.move(to: NSPoint(x: 470, y: y))
    arrow.line(to: NSPoint(x: 620, y: y))
    arrow.move(to: NSPoint(x: 580, y: y+40))
    arrow.line(to: NSPoint(x: 620, y: y))
    arrow.line(to: NSPoint(x: 580, y: y-40))
    arrow.lineWidth = 14
    arrow.lineCapStyle = .round
    arrow.lineJoinStyle = .round
    NSColor(calibratedRed: 0.88, green: 0.16, blue: 0.45, alpha: 0.85).setStroke()
    arrow.stroke()

    // No baked labels — clean minimal look (icons + arrow only). The Applications symlink is
    // blanked in build.sh (space name); the app's un-removable Finder label stays dark-on-black
    // (near-invisible) with the minimum text size.

    img.unlockFocus()
    return img
}

// ---------------------------------------------------------------- write
let dir = FileManager.default.currentDirectoryPath
savePNG(makeIcon(), to: dir + "/assets/icon_1024.png", pixelsWide: 1024, pixelsHigh: 1024)
savePNG(makeDMGBackground(), to: dir + "/assets/dmg-bg.png", pixelsWide: 1120, pixelsHigh: 680)
print("assets written: assets/icon_1024.png, assets/dmg-bg.png")
