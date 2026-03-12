# buildnectar.com

The official Nectar website — built with Nectar.

## Development

```bash
# From the repo root
nectar dev website/src/app.nectar
```

## Build

```bash
nectar build website/src/app.nectar --target ssg --output website/dist
```

## Deploy

Deployment is automatic via Cloud Build on push to main.

Manual deploy:
```bash
gcloud builds submit --config website/cloudbuild.yaml
```

## Structure

- `src/app.nectar` — App definition with router
- `src/pages/` — Page components with SEO metadata
- `src/components/` — Shared UI components
- `nectar.toml` — Project manifest
- `Dockerfile` — Cloud Run container
- `cloudbuild.yaml` — GCP Cloud Build pipeline
