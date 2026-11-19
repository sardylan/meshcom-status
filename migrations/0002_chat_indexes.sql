-- Chat: pages of messages per addressee, and direct messages per sender.
CREATE INDEX receptions_chat_dest ON receptions (dest, received_at DESC, id DESC)
    WHERE kind = 'message' AND copy = 1;
CREATE INDEX receptions_chat_origin ON receptions (origin, received_at DESC, id DESC)
    WHERE kind = 'message' AND copy = 1;
