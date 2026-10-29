// Copy buttons on code blocks. The page works without this script.
document.addEventListener("DOMContentLoaded", () => {
  document.querySelectorAll("button.copy").forEach((button) => {
    const code = button.parentElement.querySelector("pre code");
    if (!code) return;

    const select = () => {
      const range = document.createRange();
      range.selectNodeContents(code);
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    };

    button.addEventListener("click", () => {
      if (!navigator.clipboard) return select();
      navigator.clipboard.writeText(code.textContent).then(() => {
        button.textContent = button.dataset.done;
        setTimeout(() => { button.textContent = button.dataset.label; }, 1500);
      }, select);
    });
  });
});

// Screenshots open in a viewer with previous/next; without this script the links
// open the full-size image.
document.addEventListener("DOMContentLoaded", () => {
  const shots = [...document.querySelectorAll(".shot")];
  if (shots.length === 0 || typeof HTMLDialogElement !== "function") return;
  const es = document.documentElement.lang === "es";
  const dialog = document.createElement("dialog");
  dialog.className = "lightbox";
  dialog.innerHTML = `<img alt=""><div class="lightbox-bar"><p></p>
    <button type="button" data-step="-1" aria-label="${es ? "Anterior" : "Previous"}">‹</button>
    <button type="button" data-step="1" aria-label="${es ? "Siguiente" : "Next"}">›</button>
    <button type="button" data-close aria-label="${es ? "Cerrar" : "Close"}">✕</button></div>`;
  document.body.append(dialog);
  const img = dialog.querySelector("img");
  const caption = dialog.querySelector("p");
  let index = 0;

  const show = (i) => {
    index = (i + shots.length) % shots.length;
    const shot = shots[index];
    img.src = shot.querySelector("a").href;
    img.alt = shot.querySelector("img").alt;
    caption.innerHTML = shot.querySelector("figcaption").innerHTML;
  };

  shots.forEach((shot, i) => {
    shot.querySelector("a").addEventListener("click", (event) => {
      event.preventDefault();
      show(i);
      dialog.showModal();
    });
  });
  dialog.addEventListener("click", (event) => {
    const button = event.target.closest("button");
    if (button?.dataset.step) show(index + Number(button.dataset.step));
    else if (button?.hasAttribute("data-close") || event.target === dialog) dialog.close();
  });
  dialog.addEventListener("keydown", (event) => {
    if (event.key === "ArrowRight") show(index + 1);
    if (event.key === "ArrowLeft") show(index - 1);
  });
});
