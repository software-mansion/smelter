# Changesets

## Release (bumps to 1.0.0, do manually instead)

#### If repository is in pre-release mode exit with:

```
pnpm changeset pre exit
```

#### Select packages that need to updated:

```
pnpm changeset
```

- It will create changeset file in the repository, you don't need to commit it.


#### Apply all changeset files in the repository

```
pnpm changeset version
```

- It will update all package.json files with new versions.
- It will remove all changeset files.


#### Publish

```
pnpm changeset publish
```


## Pre-release

#### If repository is not in pre-release mode enter with:

```
pnpm changeset pre enter next
```

#### Select packages that need to updated:

```
pnpm changeset
```

- It will create changeset file in the repository. While in pre-release mode those files need to be commited, so they can be applied after existing pre-release mode.


#### Apply all changeset files in the repository

```
pnpm changeset version
```

- It will update all package.json files with new versions.
- It will remove all changeset files.


#### Publish

```
pnpm changeset publish
```


