ALTER TABLE user_notifications
    ADD COLUMN email_delivery_eligible BOOLEAN NOT NULL DEFAULT FALSE;
