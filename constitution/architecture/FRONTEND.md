# FRONTEND.md - Frontend Architecture

**Authority:** guidance (frontend patterns, performance, and user experience)
**Layer:** Guides
**Binding:** No
**Scope:** frontend architecture, performance optimization, and UX patterns
**Non-goals:** specific framework tutorials, visual design guidelines

---

## 1. Frontend Architecture Principles

### 1.1 Performance is User Experience
**Core Web Vitals are engineering requirements:**
- **LCP (Largest Contentful Paint):** < 2.5s
- **FID (First Input Delay):** < 100ms
- **CLS (Cumulative Layout Shift):** < 0.1

**Every 100ms delay = 1% conversion drop.**

### 1.2 Progressive Enhancement
- **Baseline:** Works without JavaScript
- **Enhancement:** Add interactivity progressively
- **Resilience:** Graceful degradation
- **Accessibility:** Works for all users

### 1.3 Mobile First
- Design for constraints first
- Progressive enhancement for desktop
- Touch-friendly targets (44px minimum)
- Responsive images and layouts

### 1.4 Accessibility (a11y)
**Not optional. Legal and ethical requirement.**
- Semantic HTML
- Keyboard navigation
- Screen reader support
- Color contrast (WCAG AA minimum)
- Focus management

---

## 2. Rendering Strategies

### 2.1 Static Site Generation (SSG)
**When to use:**
- Content that changes infrequently
- Blogs, documentation, marketing sites
- Maximum performance

**Benefits:**
- CDN cacheable
- Fastest load times
- No server required

**Examples:** Next.js SSG, Gatsby, 11ty

### 2.2 Server-Side Rendering (SSR)
**When to use:**
- Dynamic content
- SEO requirements
- Personalized content

**Benefits:**
- Fast initial load
- SEO friendly
- Dynamic data at request time

**Examples:** Next.js SSR, Nuxt, SvelteKit

### 2.3 Client-Side Rendering (SPA)
**When to use:**
- Highly interactive applications
- After initial page load
- Dashboards, admin panels

**Benefits:**
- Smooth interactions
- Reduced server load
- App-like experience

**Trade-offs:**
- Slower initial load
- SEO challenges
- More JavaScript

### 2.4 Incremental Static Regeneration (ISR)
**When to use:**
- Mostly static with some dynamic data
- High traffic pages
- Stale-while-revalidate pattern

**How it works:**
1. Serve cached static page
2. Trigger background regeneration
3. Next request gets updated page

### 2.5 Islands Architecture
**When to use:**
- Content-heavy sites
- Minimal JavaScript
- Progressive enhancement

**Concept:**
- Static HTML by default
- Interactive "islands" hydrate separately
- Reduced JavaScript footprint

**Examples:** Astro, Fresh, Eleventy + Alpine

---

## 3. State Management

### 3.1 Local State
- **useState (React):** Component-specific
- **Signals (Solid/Vue):** Fine-grained reactivity
- **When to use:** UI-only state, form inputs

### 3.2 Global State
**Options by complexity:**
- **Context API (React):** Simple, prop drilling alternative
- **Zustand:** Lightweight, no boilerplate
- **Redux:** Complex, time-travel, devtools
- **MobX:** Observable, OOP style

**When to use:**
- User authentication
- Theme preferences
- Shopping cart
- Cross-component data

### 3.3 Server State
**Libraries:**
- **React Query (TanStack Query):** Caching, synchronization
- **SWR:** Stale-while-revalidate
- **Apollo Client:** GraphQL

**Benefits:**
- Automatic caching
- Background refetching
- Optimistic updates
- Error handling

### 3.4 URL State
- **Use for:** Shareable views, filters, pagination
- **Benefits:** Bookmarkable, back button works
- **Implementation:** Query parameters, hash routing

---

## 4. Performance Optimization

### 4.1 Bundle Optimization
**Code splitting:**
- Route-based splitting
- Component lazy loading
- Dynamic imports

**Tree shaking:**
- ES modules
- Side-effect-free imports
- Dead code elimination

**Bundle analysis:**
- webpack-bundle-analyzer
- Import cost (VSCode)
- Lighthouse bundle analysis

### 4.2 Loading Strategies
**Priority:**
- **Critical:** Render-blocking, above fold
- **Important:** Needed for interactivity
- **Deferred:** Below fold, non-critical

**Techniques:**
- `preload` for critical resources
- `prefetch` for next navigation
- `lazy` for images
- `async/defer` for scripts

### 4.3 Image Optimization
- **Formats:** WebP, AVIF for modern browsers
- **Responsive:** `srcset` for different sizes
- **Lazy loading:** Native or library
- **CDN:** Image optimization services
- **Dimensions:** Always specify width/height (prevent CLS)

### 4.4 Caching Strategies
- **Service Workers:** Offline support, caching
- **Cache API:** Programmatic cache control
- **HTTP caching:** Cache-Control headers
- **Stale-while-revalidate:** Fresh data, fast loads

---

## 5. Component Architecture

### 5.1 Atomic Design
- **Atoms:** Basic building blocks (buttons, inputs)
- **Molecules:** Groups of atoms (search bar)
- **Organisms:** Complex components (header)
- **Templates:** Page layouts
- **Pages:** Specific instances

### 5.2 Container/Presentational Pattern
- **Containers:** Data fetching, business logic
- **Presentational:** Pure UI, props in, events out
- **Benefits:** Separation of concerns, testability

### 5.3 Compound Components
- Related components that share state
- Flexible composition
- Example: `<Tabs>`, `<Tab>`, `<TabPanel>`

### 5.4 Render Props vs Hooks
- **Render props:** Component injection
- **Hooks:** Logic reuse without components
- **Modern preference:** Hooks for most cases

---

## 6. API Integration

### 6.1 REST Integration
- **Fetch API:** Native, promises
- **Axios:** Interceptors, timeouts, wider browser support
- **Error handling:** Global and local
- **Loading states:** Skeletons, spinners

### 6.2 GraphQL Integration
- **Apollo Client:** Caching, optimistic UI
- **Relay:** Facebook's GraphQL client
- **urql:** Lightweight alternative

**Benefits:**
- Precise data fetching
- Single endpoint
- Strong typing

### 6.3 Real-Time
- **WebSockets:** Bidirectional, persistent
- **SSE (Server-Sent Events):** Server to client
- **Polling:** Simple, less efficient
- **Subscriptions:** GraphQL real-time

---

## 7. Testing

### 7.1 Unit Testing
- **Jest:** JavaScript testing framework
- **Vitest:** Fast, Vite-native
- **React Testing Library:** User-centric testing

**What to test:**
- Pure functions
- Component rendering
- User interactions
- Edge cases

### 7.2 Integration Testing
- **Cypress:** E2E testing
- **Playwright:** Cross-browser E2E
- **Testing Library:** Component integration

**What to test:**
- User flows
- API integration
- State management

### 7.3 Visual Testing
- **Storybook:** Component development
- **Chromatic:** Visual regression
- **Percy:** Screenshot comparison

### 7.4 Performance Testing
- **Lighthouse:** Automated audits
- **WebPageTest:** Real device testing
- **React Profiler:** Component performance

---

## 8. Build and Deployment

### 8.1 Build Tools
- **Vite:** Fast, modern
- **Webpack:** Mature, configurable
- **esbuild:** Go-based, extremely fast
- **Turbopack:** Rust-based, Webpack successor

### 8.2 TypeScript
- **Benefits:** Type safety, IDE support, documentation
- **Strict mode:** Catch more errors
- **Gradual adoption:** jsdoc, allowJs

### 8.3 CI/CD
- **Linting:** ESLint, Prettier
- **Type checking:** tsc --noEmit
- **Testing:** Unit, integration, e2e
- **Building:** Production optimizations
- **Deployment:** Vercel, Netlify, Cloudflare Pages

---

## 9. Anti-Patterns

- **Giant bundles:** No code splitting
- **Prop drilling:** Deep component nesting
- **No error boundaries:** Crash entire app
- **Synchronous blocking:** Main thread hogging
- **Memory leaks:** Unsubscribed listeners
- **No loading states:** Blank screens
- **Layout shift:** No dimensions on images
- **Blocking CSS/JS:** Render-blocking resources
- **No accessibility:** Missing ARIA, keyboard nav
- **Over-engineering:** Complex solutions for simple problems

---

## Links

- `methodology/ARCHITECTURE.md` - CTO-level architecture doctrine
- `architecture/WEB.md` - Web architecture
- `architecture/CACHING.md` - Caching strategies
- `architecture/SECURITY.md` - Frontend security
- `architecture/PERFORMANCE.md` - Performance patterns
