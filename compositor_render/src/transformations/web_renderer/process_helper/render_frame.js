function smelter_renderFrame(sourceId, buffer, width, height) {
    const canvas = document.getElementById(sourceId);
    const ctx = canvas.getContext("2d");
    const imageData = new ImageData(new Uint8ClampedArray(buffer), width, height);

    canvas.width = width;
    canvas.height = height;
    ctx.putImageData(imageData, 0, 0);
}
