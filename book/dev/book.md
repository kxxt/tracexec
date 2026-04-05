# Maintaining this Book


## Adding a Video

When adding a video to the book, please use the following HTML snippet.
It ensures that the video is lazily loaded and has controls.

```md
<video
  src="XXX" controls preload="none" loading="lazy"
  poster="../assets/gdb-launcher-cover.jpg"
  width="100%">
</video>
```

Please avoid storing videos inside the git repository unless it is below 5MiB.
Currently, we post the videos in a GitHub discussion thread and then reference them by URL in the book.

Please add a cover image for the video by taking an image snapshot of the video
at a suitable moment.
This can be done by right clicking the video and select `Take Snapshot` in Firefox.
The cover image should be stored in the git repository.
