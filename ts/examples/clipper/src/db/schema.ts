import { sql } from 'drizzle-orm';
import { int, text, sqliteTable } from 'drizzle-orm/sqlite-core';

/** `clip_jobs` table stores clip jobs to be processed. */
export const clipJobsTable = sqliteTable('clip_jobs', {
  id: int().primaryKey({ autoIncrement: true }),
  status: text({ enum: ['pending', 'done', 'corrupted'] })
    .default('pending')
    .notNull(),
  clipTimestamp: int().notNull(),
  duration: int().notNull(),
  createdAt: text().default(sql`(CURRENT_TIMESTAMP)`),
  updatedAt: text().default(sql`(CURRENT_TIMESTAMP)`),
});
