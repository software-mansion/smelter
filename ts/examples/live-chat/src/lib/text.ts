/** Count number of lines required to display given text. */
export function countLines(
    text: string,
    maxWidth: number,
    font: opentype.Font,
    fontSize: number,
): number {
    const words = text.split(/\s+/);
    let lines = 1;
    let currentLine = "";

    for (let i = 0; i < words.length; i++) {
        const word = words[i];
        const testLine = currentLine ? currentLine + " " + word : word;
        const width = font.getAdvanceWidth(testLine, fontSize);

        if (width <= maxWidth) {
            currentLine = testLine;
        } else {
            // If the word alone is too long, just put it on its own line
            const wordWidth = font.getAdvanceWidth(word, fontSize);
            if (wordWidth > maxWidth) {
                lines++;
                currentLine = ""; // start fresh
            } else {
                lines++;
                currentLine = word;
            }
        }
    }

    return lines;
}
