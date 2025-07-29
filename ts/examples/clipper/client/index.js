const root = document.body;

const State = {
  clips: [],
  selectedClip: '',
  loadClips: () => {
    return m
      .request({
        method: 'GET',
        url: '/api/v1/clips',
      })
      .then(clips => {
        State.clips = clips;
      });
  },
};

const clipStatusMap = {
  done: 'Done',
  pending: 'Pending',
  corrupted: 'Corrupted',
};

const ClipsList = {
  oninit: State.loadClips,
  view: () => {
    return m('.clips-list', [
      m('.clip-list-row', [
        m('.clip-list-col.clip-list-header', m('span', 'Name')),
        m('.clip-list-col.clip-list-header', m('span', 'Actions')),
      ]),
      ...State.clips.map(clip =>
        m('.clip-list-row', [
          m('.clip-list-col', m('span', clip.name)),
          m(
            '.clip-list-col',
            clip.status === 'done'
              ? m(
                  'button',
                  {
                    class: 'clip-item-action',
                    onclick: () => (State.selectedClip = `/static/${clip.filename}`),
                  },
                  'Watch'
                )
              : m('span', { class: 'clip-item-action', disabled: true }, clipStatusMap[clip.status])
          ),
        ])
      ),
    ]);
  },
};

const Main = {
  view: () => {
    return m('main', { class: 'main' }, [
      m('video', { class: 'video', controls: true, src: State.selectedClip }),
      m(ClipsList),
    ]);
  },
};

m.mount(root, Main);
