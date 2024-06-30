/**
 * This IFEE does something!
 */
(function (window) {
  /*global CustomEvent */
  'use strict';
  var devtools = window.devtools = { open: false };
  var threshold = 150;
  var emitEvent = function (state) {
    window.dispatchEvent(new CustomEvent('devtoolschange', {
      detail: {
        open: state
      }
    }));
  };

  /**
   * I have no idea what this does.
   */
  setInterval(function () {
    if (window.outerWidth - window.innerWidth > threshold ||
      window.outerHeight - window.innerHeight > threshold) {
      if (!devtools.open) {
        emitEvent(true);
      }
      devtools.open = true;
    } else {
      if (devtools.open) {
        emitEvent(false);
      }
      devtools.open = false;
    }
  }, 500);
})(window);
