import { Text, View, ViewProps } from "@swmansion/smelter";
import { useContext, useMemo, useState } from "react";
import { FadeInShader } from "../../components/fade-in-shader";
import { type TMessage, useChatStub } from "../../hooks/use-chat-stub";
import { useFont } from "../../hooks/use-font";
import { countLines } from "./../../lib";
import { ChatContext, useChat } from "./context";

export function Chat() {
    const { style } = useChat();
    const [messages, setMessages] = useState<Array<TMessage>>([]);

    useChatStub({
        onMessage: (message) =>
            void setMessages((old) => [
                // Check how many single line messages can fit the screen.
                ...(old.length < Math.ceil(1080 / style.lineHeight)
                    ? old
                    : old.slice(1)),
                message,
            ]),
    });

    return (
        <>
            <Background />
            <MessagesList messages={messages} />
        </>
    );
}

function Background() {
    const { style } = useChat();
    const totalWidth =
        style.messageWidth + style.usernameWidth + style.padding * 2;

    return (
        <View
            style={{
                width: totalWidth,
                height: 1080,
                backgroundColor: "#17171c",
            }}
        />
    );
}

type MessagesProps = {
    messages: Array<TMessage>;
};

const messageHeightCache: Record<string, number> = {};

export function MessagesList({ messages }: MessagesProps) {
    const font = useFont({ path: "./assets/JetBrainsMonoNL-Regular.ttf" });
    const { style: chatStyle } = useContext(ChatContext);

    const renderedMessages = useMemo(() => {
        if (!font) {
            return null;
        }

        return messages.map((message, i) => {
            const lines =
                messageHeightCache[message.id] ??
                countLines(
                    message.text,
                    chatStyle.messageWidth,
                    font,
                    chatStyle.fontSize,
                );

            if (!messageHeightCache[message.id]) {
                messageHeightCache[message.id] = lines;
            }

            const height = lines * chatStyle.lineHeight;

            return (
                <Message
                    key={message.id}
                    message={message}
                    fadeIn={i === messages.length - 1}
                    height={height}
                />
            );
        });
    }, [font, messages]);

    return (
        <View
            style={{
                bottom: chatStyle.padding,
                left: 0,
                direction: "column",
                height: 5000,
            }}
        >
            <View></View>
            {renderedMessages}
        </View>
    );
}

export type MessageProps = {
    message: TMessage;
    height: number;
    /** If the message should run fade-in animation. */
    fadeIn?: boolean;
} & ViewProps;

/** Displays a single chat message. */
export function Message({
    message,
    height,
    fadeIn = false,
    style,
    ...props
}: MessageProps) {
    const { style: chatStyle } = useChat();

    const totalWidth = chatStyle.messageWidth + chatStyle.usernameWidth + 5;

    return (
        <View
            key={message.id}
            style={{
                paddingLeft: chatStyle.padding,
                width: totalWidth,
                height,
                ...style,
            }}
            {...props}
        >
            <FadeInShader
                duration={200}
                disabled={!fadeIn}
                resolution={{ width: totalWidth, height }}
            >
                <View
                    style={{
                        width: totalWidth,
                        height,
                    }}
                >
                    <View
                        style={{
                            paddingRight: 5,
                            width: chatStyle.usernameWidth,
                            height: height,
                        }}
                    >
                        <Text
                            style={{
                                fontSize: chatStyle.fontSize,
                                fontFamily: "JetBrains Mono NL",
                                lineHeight: chatStyle.lineHeight,
                                wrap: "glyph",
                                color: message.color,
                                width: chatStyle.usernameWidth,
                                height,
                            }}
                        >
                            {message.user}
                        </Text>
                    </View>
                    <Text
                        key={message.id}
                        style={{
                            fontSize: chatStyle.fontSize,
                            fontFamily: "JetBrains Mono NL",
                            color: "#ffffff",
                            wrap: "word",
                            height,
                            width: chatStyle.messageWidth,
                            lineHeight: chatStyle.lineHeight,
                        }}
                    >
                        {message.text}
                    </Text>
                </View>
            </FadeInShader>
        </View>
    );
}
