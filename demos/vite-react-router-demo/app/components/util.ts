export const getNewOutputId = (() => {
  let counter = 1;
  return () => {
    const outputId = `output-${counter}`;
    counter += 1;
    return outputId;
  };
})();
