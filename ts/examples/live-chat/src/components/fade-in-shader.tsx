import { Shader, ShaderProps } from "@swmansion/smelter";
import { useEffect, useState } from "react";

type FadeInShaderProps = {
    duration: number;
    children: React.ReactNode | Array<React.ReactNode>;
    disabled?: boolean;
} & Omit<ShaderProps, "shaderId">;

/**
 * Fade in shader.
 * After animation ends renders original content
 * to remove any blending artefacts.
 */
export function FadeInShader({
    duration,
    disabled = false,
    children,
    ...props
}: FadeInShaderProps) {
    const [time, setTime] = useState<number>(0);
    const [running, setRunning] = useState<boolean>(!disabled);

    useEffect(() => {
        const id = setInterval(() => {
            setTime((old) => {
                if (old === duration) {
                    clearInterval(id);
                    setRunning(false);
                    return old;
                }

                return old + 1;
            });
        }, 1);

        return () => clearInterval(id);
    }, []);

    return running ? (
        <Shader
            shaderId="fade-in"
            shaderParam={{
                type: "struct",
                value: [
                    { type: "f32", fieldName: "local_time", value: time },
                    { type: "f32", fieldName: "duration", value: duration },
                ],
            }}
            {...props}
        >
            {children}
        </Shader>
    ) : (
        children
    );
}
