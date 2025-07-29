import { createContext, useContext } from "react";

export type ChatStyle = {
    fontSize: number;
    lineHeight: number;
    messageWidth: number;
    usernameWidth: number;
    padding: number;
};

export type ChatContextValue = {
    style: ChatStyle;
};

const defaultValue: ChatContextValue = {
    style: {
        fontSize: 20,
        lineHeight: 28,
        messageWidth: 400,
        usernameWidth: 150,
        padding: 28,
    },
};

export const ChatContext = createContext<ChatContextValue>(defaultValue);

type ChatContextProviderProps = {
    style?: Partial<ChatStyle>;
    children: React.ReactNode;
};

export function ChatContextProvider({
    style = {},
    children,
}: ChatContextProviderProps) {
    return (
        <ChatContext.Provider
            value={{
                style: {
                    ...defaultValue.style,
                    ...style,
                },
            }}
        >
            {children}
        </ChatContext.Provider>
    );
}

export function useChat() {
    return useContext(ChatContext);
}
