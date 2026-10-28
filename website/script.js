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
