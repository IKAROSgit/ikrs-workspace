import { z } from "zod";

const dateSchema = z.coerce.date();

export const ConsultantSchema = z.object({
  _v: z.literal(1),
  id: z.string().min(1),
  email: z.string().email(),
  name: z.string().min(1),
  role: z.enum(["consultant", "lead", "admin"]),
  preferences: z.object({
    theme: z.enum(["dark", "light"]),
    terminal: z.string(),
    timezone: z.string(),
  }),
  createdAt: dateSchema,
  updatedAt: dateSchema,
});

export const ClientSchema = z.object({
  _v: z.literal(1),
  id: z.string().min(1),
  name: z.string().min(1),
  domain: z.string().min(1),
  slug: z.string().regex(/^[a-z0-9-]+$/),
  branding: z.object({
    logo: z.string().optional(),
    primaryColor: z.string().optional(),
    secondaryColor: z.string().optional(),
  }),
  createdAt: dateSchema,
  updatedAt: dateSchema,
});

export const EngagementSchema = z.object({
  _v: z.literal(1),
  id: z.string().min(1),
  consultantId: z.string().min(1),
  clientId: z.string().min(1),
  status: z.enum(["active", "paused", "completed", "archived"]),
  startDate: dateSchema,
  endDate: dateSchema.optional(),
  settings: z.object({
    timezone: z.string(),
    billingRate: z.number().positive().optional(),
    description: z.string().optional(),
  }),
  vault: z.object({
    path: z.string().min(1),
    archivePath: z.string().optional(),
    status: z.enum(["active", "archived", "deleted"]),
  }),
  createdAt: dateSchema,
  updatedAt: dateSchema,
});

export const CredentialSchema = z.object({
  _v: z.literal(1),
  id: z.string().min(1),
  engagementId: z.string().min(1),
  provider: z.literal("google"),
  accountEmail: z.string().email(),
  scopes: z.array(z.string()),
  keychainKey: z.string().min(1),
  lastRefreshed: dateSchema,
  status: z.enum(["active", "expired", "revoked"]),
  createdAt: dateSchema,
});

const SubtaskSchema = z.object({
  title: z.string().min(1),
  done: z.boolean(),
});

export const TaskSchema = z.object({
  _v: z.literal(1),
  id: z.string().min(1),
  engagementId: z.string().min(1),
  title: z.string().min(1),
  description: z.string().optional(),
  status: z.enum(["todo", "in_progress", "done"]),
  priority: z.enum(["p1", "p2", "p3"]),
  dueDate: dateSchema.optional(),
  tags: z.array(z.string()),
  subtasks: z.array(SubtaskSchema),
  sortOrder: z.number(),
  source: z.enum(["manual", "imported"]),
  createdAt: dateSchema,
  updatedAt: dateSchema,
});
