import { useEffect, useState } from "react";
import fs from "node:fs/promises";
import opentype from "opentype.js";

type UseFontOpts = {
    /** Path to the font file. */
    path: string;
};

const registeredFonts: Record<string, opentype.Font> = {};

/** Loads font with opentype. */
export function useFont({ path }: UseFontOpts) {
    const [font, setFont] = useState<opentype.Font | undefined>(
        registeredFonts[path],
    );

    useEffect(() => {
        if (font) {
            return;
        }

        let cancel = false;

        (async () => {
            try {
                const buffer = await fs.readFile(
                    "./assets/JetBrainsMonoNL-Regular.ttf",
                );

                const arrayBuffer = buffer.buffer.slice(
                    buffer.byteOffset,
                    buffer.byteOffset + buffer.byteLength,
                );

                const font = opentype.parse(arrayBuffer);

                if (!cancel) {
                    setFont(font);
                    registeredFonts[path] = font;
                }
            } catch (err) {
                console.error("Failed to load font.", err);
            }
        })();

        return () => {
            cancel = true;
        };
    }, []);

    return font;
}
