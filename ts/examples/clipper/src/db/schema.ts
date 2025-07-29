import { sql } from 'drizzle-orm';
import { int, text, sqliteTable } from 'drizzle-orm/sqlite-core';

export const clipsTable = sqliteTable('clips', {
  id: int().primaryKey({ autoIncrement: true }),
  name: text().notNull(),
  status: text({ enum: ['pending', 'done', 'corrupted'] })
    .default('pending')
    .notNull(),
  clipTimestamp: int().notNull(),
  filename: text(),
  duration: int().notNull(),
  createdAt: text()
    .notNull()
    .default(sql`(CURRENT_TIMESTAMP)`),
  updatedAt: text()
    .notNull()
    .default(sql`(CURRENT_TIMESTAMP)`),
});
